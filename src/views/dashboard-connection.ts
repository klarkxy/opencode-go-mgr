export function maskConnectionKey(key: string): string {
  if (!key) return "未设置";
  if (key.length <= 8) return key;
  return `${key.slice(0, 4)}…${key.slice(-4)}`;
}

export async function writeConnectionValue(
  writeText: ((value: string) => Promise<void>) | undefined,
  value: string,
): Promise<void> {
  if (!value) throw new Error("没有可复制的内容");
  if (!writeText) throw new Error("当前环境不支持剪贴板");
  await writeText(value);
}

