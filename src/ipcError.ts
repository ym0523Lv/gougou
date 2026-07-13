export function ipcErrorMessage(error: unknown, fallback: string) {
  console.error(fallback, error);
  if (!import.meta.env.DEV) return fallback;

  if (typeof error === "object" && error !== null) {
    const code = "code" in error ? String(error.code) : "unknown_error";
    const message = "message" in error ? String(error.message) : String(error);
    return `${fallback}（${code}: ${message}）`;
  }
  return `${fallback}（${String(error)}）`;
}
