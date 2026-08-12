use std::io::{Read, SeekFrom};
use std::marker::PhantomData;
use std::mem::{size_of, ManuallyDrop};
use std::rc::Rc;
use std::sync::Mutex;
use std::thread::ThreadId;

use windows::core::{implement, ComObject, Error, IUnknown, Interface, Ref, Result, BOOL, HRESULT};
use windows::Win32::Foundation::{
    DATA_S_SAMEFORMATETC, DV_E_CLIPFORMAT, DV_E_DVASPECT, DV_E_DVTARGETDEVICE, DV_E_LINDEX,
    DV_E_TYMED, E_ACCESSDENIED, E_NOTIMPL, E_POINTER, OLE_E_ADVISENOTSUPPORTED, OLE_E_NOCONNECTION,
    S_FALSE, S_OK,
};
use windows::Win32::System::Com::{
    IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, ISequentialStream_Impl, IStream,
    IStream_Impl, DATADIR_GET, DVASPECT_CONTENT, FORMATETC, LOCKTYPE, STATFLAG, STATSTG, STGC,
    STGMEDIUM, STGMEDIUM_0, STGM_READ, STGTY_STREAM, STREAM_SEEK, STREAM_SEEK_CUR, STREAM_SEEK_END,
    STREAM_SEEK_SET, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    OleGetClipboard, OleInitialize, OleSetClipboard, OleUninitialize, DROPEFFECT_COPY,
};
use windows::Win32::UI::Shell::{
    SHCreateStdEnumFmtEtc, CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, CFSTR_PREFERREDDROPEFFECT,
    FD_FILESIZE, FD_PROGRESSUI, FD_UNICODE, FILEDESCRIPTORW, FILEGROUPDESCRIPTORW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

use crate::virtual_file::ReadSeek;
use crate::{ClipboardError, VirtualFile};

const FORMAT_INDEX_NONE: i32 = -1;

#[derive(Clone, Copy)]
struct ClipboardFormats {
    descriptor: u16,
    contents: u16,
    preferred_drop_effect: u16,
}

impl ClipboardFormats {
    fn register() -> Result<Self> {
        Ok(Self {
            descriptor: register_format(CFSTR_FILEDESCRIPTORW)?,
            contents: register_format(CFSTR_FILECONTENTS)?,
            preferred_drop_effect: register_format(CFSTR_PREFERREDDROPEFFECT)?,
        })
    }

    fn as_format_etc(self) -> [FORMATETC; 3] {
        [
            FORMATETC {
                cfFormat: self.descriptor,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: FORMAT_INDEX_NONE,
                tymed: TYMED_HGLOBAL.0 as u32,
            },
            FORMATETC {
                cfFormat: self.contents,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: 0,
                tymed: TYMED_ISTREAM.0 as u32,
            },
            FORMATETC {
                cfFormat: self.preferred_drop_effect,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: FORMAT_INDEX_NONE,
                tymed: TYMED_HGLOBAL.0 as u32,
            },
        ]
    }
}

fn register_format(name: windows::core::PCWSTR) -> Result<u16> {
    let format = unsafe { RegisterClipboardFormatW(name) };
    if format == 0 || format > u16::MAX as u32 {
        Err(Error::from_win32())
    } else {
        Ok(format as u16)
    }
}

enum RequestedFormat {
    Descriptor,
    Contents,
    PreferredDropEffect,
}

#[implement(IDataObject)]
struct VirtualFileDataObject {
    file: VirtualFile,
    formats: ClipboardFormats,
}

impl VirtualFileDataObject {
    fn requested_format(
        &self,
        format: *const FORMATETC,
    ) -> std::result::Result<RequestedFormat, HRESULT> {
        if format.is_null() {
            return Err(E_POINTER);
        }
        let format = unsafe { &*format };
        if !format.ptd.is_null() {
            return Err(DV_E_DVTARGETDEVICE);
        }
        if format.dwAspect != DVASPECT_CONTENT.0 {
            return Err(DV_E_DVASPECT);
        }

        let (kind, expected_lindex, expected_tymed) = if format.cfFormat == self.formats.descriptor
        {
            (
                RequestedFormat::Descriptor,
                FORMAT_INDEX_NONE,
                TYMED_HGLOBAL.0 as u32,
            )
        } else if format.cfFormat == self.formats.contents {
            (RequestedFormat::Contents, 0, TYMED_ISTREAM.0 as u32)
        } else if format.cfFormat == self.formats.preferred_drop_effect {
            (
                RequestedFormat::PreferredDropEffect,
                FORMAT_INDEX_NONE,
                TYMED_HGLOBAL.0 as u32,
            )
        } else {
            return Err(DV_E_CLIPFORMAT);
        };
        if format.lindex != expected_lindex {
            return Err(DV_E_LINDEX);
        }
        if format.tymed & expected_tymed == 0 {
            return Err(DV_E_TYMED);
        }
        Ok(kind)
    }

    fn descriptor_medium(&self) -> Result<STGMEDIUM> {
        let mut descriptor = FILEDESCRIPTORW {
            dwFlags: (FD_FILESIZE.0 | FD_PROGRESSUI.0 | FD_UNICODE.0) as u32,
            nFileSizeHigh: (self.file.size() >> 32) as u32,
            nFileSizeLow: self.file.size() as u32,
            ..Default::default()
        };
        let name = self.file.file_name_utf16();
        let mut file_name = [0_u16; 260];
        file_name[..name.len()].copy_from_slice(name);
        descriptor.cFileName = file_name;
        let group = FILEGROUPDESCRIPTORW {
            cItems: 1,
            fgd: [descriptor],
        };
        hglobal_medium_from_copy(&group)
    }

    fn preferred_drop_effect_medium(&self) -> Result<STGMEDIUM> {
        hglobal_medium_from_copy(&DROPEFFECT_COPY.0)
    }
}

impl IDataObject_Impl for VirtualFileDataObject_Impl {
    fn GetData(&self, format: *const FORMATETC) -> Result<STGMEDIUM> {
        match self.requested_format(format).map_err(Error::from_hresult)? {
            RequestedFormat::Descriptor => self.descriptor_medium(),
            RequestedFormat::PreferredDropEffect => self.preferred_drop_effect_medium(),
            RequestedFormat::Contents => {
                let reader = self.file.open_content().map_err(|err| {
                    Error::new(windows::Win32::Foundation::STG_E_READFAULT, err.to_string())
                })?;
                let stream = ComObject::new(ReadSeekStream::new(reader, self.file.size()))
                    .into_interface::<IStream>();
                Ok(STGMEDIUM {
                    tymed: TYMED_ISTREAM.0 as u32,
                    u: STGMEDIUM_0 {
                        pstm: ManuallyDrop::new(Some(stream)),
                    },
                    pUnkForRelease: ManuallyDrop::new(None),
                })
            }
        }
    }

    fn GetDataHere(&self, _format: *const FORMATETC, _medium: *mut STGMEDIUM) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        match self.requested_format(format) {
            Ok(_) => S_OK,
            Err(err) => err,
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        format_in: *const FORMATETC,
        format_out: *mut FORMATETC,
    ) -> HRESULT {
        if format_in.is_null() || format_out.is_null() {
            return E_POINTER;
        }
        unsafe {
            *format_out = *format_in;
            (*format_out).ptd = std::ptr::null_mut();
        }
        DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self,
        _format: *const FORMATETC,
        _medium: *const STGMEDIUM,
        _release: BOOL,
    ) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn EnumFormatEtc(&self, direction: u32) -> Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(Error::from_hresult(E_NOTIMPL));
        }
        unsafe { SHCreateStdEnumFmtEtc(&self.formats.as_format_etc()) }
    }

    fn DAdvise(
        &self,
        _format: *const FORMATETC,
        _advf: u32,
        _sink: Ref<'_, IAdviseSink>,
    ) -> Result<u32> {
        Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }

    fn DUnadvise(&self, _connection: u32) -> Result<()> {
        Err(Error::from_hresult(OLE_E_NOCONNECTION))
    }

    fn EnumDAdvise(&self) -> Result<windows::Win32::System::Com::IEnumSTATDATA> {
        Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }
}

#[implement(IStream)]
struct ReadSeekStream {
    reader: Mutex<Box<dyn ReadSeek + Send>>,
    size: u64,
}

impl ReadSeekStream {
    fn new(reader: Box<dyn ReadSeek + Send>, size: u64) -> Self {
        Self {
            reader: Mutex::new(reader),
            size,
        }
    }
}

impl ISequentialStream_Impl for ReadSeekStream_Impl {
    fn Read(&self, buffer: *mut core::ffi::c_void, count: u32, read: *mut u32) -> HRESULT {
        if !read.is_null() {
            unsafe { *read = 0 };
        }
        if buffer.is_null() && count != 0 {
            return E_POINTER;
        }
        if count == 0 {
            return S_OK;
        }
        let output = unsafe { std::slice::from_raw_parts_mut(buffer.cast::<u8>(), count as usize) };
        let mut reader = match self.reader.lock() {
            Ok(reader) => reader,
            Err(_) => return windows::Win32::Foundation::STG_E_READFAULT,
        };
        let mut bytes_read = 0;
        while bytes_read < output.len() {
            match reader.read(&mut output[bytes_read..]) {
                Ok(0) => break,
                Ok(bytes) => bytes_read += bytes,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    if !read.is_null() {
                        unsafe { *read = bytes_read as u32 };
                    }
                    return windows::Win32::Foundation::STG_E_READFAULT;
                }
            }
        }
        if !read.is_null() {
            unsafe { *read = bytes_read as u32 };
        }
        if bytes_read < count as usize {
            S_FALSE
        } else {
            S_OK
        }
    }

    fn Write(&self, _buffer: *const core::ffi::c_void, _count: u32, written: *mut u32) -> HRESULT {
        if !written.is_null() {
            unsafe { *written = 0 };
        }
        E_ACCESSDENIED
    }
}

impl IStream_Impl for ReadSeekStream_Impl {
    fn Seek(&self, distance: i64, origin: STREAM_SEEK, position: *mut u64) -> Result<()> {
        let seek_from = match origin {
            STREAM_SEEK_SET => {
                if distance < 0 {
                    return Err(Error::from_hresult(
                        windows::Win32::Foundation::STG_E_SEEKERROR,
                    ));
                }
                SeekFrom::Start(distance as u64)
            }
            STREAM_SEEK_CUR => SeekFrom::Current(distance),
            STREAM_SEEK_END => SeekFrom::End(distance),
            _ => {
                return Err(Error::from_hresult(
                    windows::Win32::Foundation::STG_E_INVALIDFUNCTION,
                ))
            }
        };
        let mut reader = self
            .reader
            .lock()
            .map_err(|_| Error::from_hresult(windows::Win32::Foundation::STG_E_SEEKERROR))?;
        let new_position = reader
            .seek(seek_from)
            .map_err(|_| Error::from_hresult(windows::Win32::Foundation::STG_E_SEEKERROR))?;
        if !position.is_null() {
            unsafe { *position = new_position };
        }
        Ok(())
    }

    fn SetSize(&self, _size: u64) -> Result<()> {
        Err(Error::from_hresult(E_ACCESSDENIED))
    }

    fn CopyTo(
        &self,
        _stream: Ref<'_, IStream>,
        _count: u64,
        _read: *mut u64,
        _written: *mut u64,
    ) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn Commit(&self, _flags: &STGC) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn Revert(&self) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn LockRegion(&self, _offset: u64, _count: u64, _lock_type: &LOCKTYPE) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn UnlockRegion(&self, _offset: u64, _count: u64, _lock_type: u32) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn Stat(&self, stat: *mut STATSTG, _flags: &STATFLAG) -> Result<()> {
        if stat.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        unsafe {
            *stat = STATSTG {
                pwcsName: windows::core::PWSTR::null(),
                r#type: STGTY_STREAM.0 as u32,
                cbSize: self.size,
                mtime: Default::default(),
                ctime: Default::default(),
                atime: Default::default(),
                grfMode: STGM_READ,
                grfLocksSupported: 0,
                clsid: Default::default(),
                grfStateBits: 0,
                reserved: 0,
            };
        }
        Ok(())
    }

    fn Clone(&self) -> Result<IStream> {
        Err(Error::from_hresult(E_NOTIMPL))
    }
}

fn hglobal_medium_from_copy<T: Copy>(value: &T) -> Result<STGMEDIUM> {
    let bytes = size_of::<T>();
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }?;
    let destination = unsafe { GlobalLock(handle) };
    if destination.is_null() {
        let _ = unsafe { windows::Win32::Foundation::GlobalFree(Some(handle)) };
        return Err(Error::from_win32());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(
            value as *const T as *const u8,
            destination.cast::<u8>(),
            bytes,
        );
        let _ = GlobalUnlock(handle);
    }
    Ok(STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: handle },
        pUnkForRelease: ManuallyDrop::new(None),
    })
}

#[must_use = "dropping the guard removes this virtual file from the clipboard"]
pub struct VirtualFileClipboard {
    object: Option<IDataObject>,
    owner_thread: ThreadId,
    _not_send: PhantomData<Rc<()>>,
}

impl VirtualFileClipboard {
    pub fn pump_messages(&self) -> std::result::Result<(), ClipboardError> {
        if std::thread::current().id() != self.owner_thread {
            return Err(ClipboardError::Backend(
                "virtual file clipboard messages must be pumped on the owner thread".into(),
            ));
        }
        pump_virtual_file_messages();
        Ok(())
    }
}

impl Drop for VirtualFileClipboard {
    fn drop(&mut self) {
        if std::thread::current().id() != self.owner_thread {
            return;
        }
        unsafe {
            if let Some(object) = self.object.as_ref() {
                if is_current_clipboard(object) {
                    let _ = OleSetClipboard(None::<&IDataObject>);
                }
            }
            drop(self.object.take());
            OleUninitialize();
        }
    }
}

fn is_current_clipboard(object: &IDataObject) -> bool {
    let Ok(current) = (unsafe { OleGetClipboard() }) else {
        return false;
    };
    let (Ok(current_identity), Ok(object_identity)) =
        (current.cast::<IUnknown>(), object.cast::<IUnknown>())
    else {
        return false;
    };
    current_identity.as_raw() == object_identity.as_raw()
}

pub fn publish_virtual_file(
    file: VirtualFile,
) -> std::result::Result<VirtualFileClipboard, ClipboardError> {
    unsafe { OleInitialize(None) }.map_err(|err| ClipboardError::Backend(err.to_string()))?;
    let formats = match ClipboardFormats::register() {
        Ok(formats) => formats,
        Err(err) => {
            unsafe { OleUninitialize() };
            return Err(ClipboardError::Backend(err.to_string()));
        }
    };
    let object =
        ComObject::new(VirtualFileDataObject { file, formats }).into_interface::<IDataObject>();
    if let Err(err) = unsafe { OleSetClipboard(&object) } {
        drop(object);
        unsafe { OleUninitialize() };
        return Err(ClipboardError::Backend(err.to_string()));
    }
    Ok(VirtualFileClipboard {
        object: Some(object),
        owner_thread: std::thread::current().id(),
        _not_send: PhantomData,
    })
}

pub fn pump_virtual_file_messages() {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
