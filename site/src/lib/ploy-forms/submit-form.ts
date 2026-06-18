export async function submitForm(
  formName: string,
  data: Record<string, string>,
): Promise<{ ok: boolean }> {
  const response = await fetch("/_ploy/form-submit", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ formName, pageUrl: window.location.href, data }),
    keepalive: true,
  });
  if (!response.ok)
    throw new Error(`Form submission failed (${response.status})`);
  return { ok: true };
}
