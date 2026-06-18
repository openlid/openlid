export function sitePath(path = "/") {
  const basePath = import.meta.env.BASE_URL.replace(/\/$/, "");
  const normalized = path.startsWith("/") ? path : `/${path}`;

  if (!basePath) return normalized;
  if (normalized === "/") return `${basePath}/`;

  return `${basePath}${normalized}`;
}
