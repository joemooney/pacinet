const AUTH_KEY = 'pacinet_api_key';

export function getApiKey(): string | null {
  return localStorage.getItem(AUTH_KEY);
}

export function setApiKey(key: string) {
  localStorage.setItem(AUTH_KEY, key);
}

export function clearApiKey() {
  localStorage.removeItem(AUTH_KEY);
}

export async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options?.headers as Record<string, string>),
  };

  const key = getApiKey();
  if (key) {
    headers['Authorization'] = `Bearer ${key}`;
  }

  const res = await fetch(path, { ...options, headers });

  if (res.status === 401) {
    window.dispatchEvent(new CustomEvent('pacinet:auth-required'));
    throw new Error('Authentication required');
  }

  if (!res.ok) {
    const err = await res.text();
    throw new Error(err || res.statusText);
  }
  return res.json();
}
