export const mockDevices = {
  local: { name: '小新 Pro', os: 'Ubuntu', kind: '本机' as const },
  peer: { name: '书房电脑', os: 'Windows 10', kind: '对端' as const },
}

export const mockClipboard = {
  type: '文本' as const,
  preview: '下周一起对齐 M590Bridge MVP 范围与验收标准…',
  meta: '来自 Windows · 12 秒前同步',
}

export const mockHistory = [
  { id: '1', type: '文本' as const, direction: '本机 → 对端', time: '刚刚', detail: '下周一起对齐…' },
  { id: '2', type: '图片' as const, direction: '对端 → 本机', time: '1 分钟前', detail: '截图.png · 2.1MB' },
  { id: '3', type: '文件' as const, direction: '本机 → 对端', time: '5 分钟前', detail: '设计稿.fig 等 2 个 · 242MB' },
  { id: '4', type: '文本' as const, direction: '对端 → 本机', time: '12 分钟前', detail: '会议纪要草稿' },
  { id: '5', type: '图片' as const, direction: '本机 → 对端', time: '28 分钟前', detail: 'wireframe.png · 840KB' },
]

export const mockTransfer = {
  title: '正在传输到 书房电脑',
  files: [
    { name: '设计稿.fig', size: '240MB', progress: 72 },
    { name: '截图.png', size: '2.1MB', progress: 100 },
  ],
  totalProgress: 67,
  speed: '92 MB/s',
  eta: '00:08',
}

export const pairCode = '482 915'
