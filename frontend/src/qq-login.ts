type Reply = { attemptId?: number; image?: string; message?: string; connected?: boolean; expired?: boolean; cancelled?: boolean };
type Call = (operation: string, body?: object) => Promise<Reply>;
type Events = { image: (image: string) => void; message: (message: string) => void; connected: () => void };

export function validationMessage(status: { connected: boolean; connectionState?: string; connectionNotice?: string | null }) {
  if (status.connectionState === 'checking') return '连接仍在验证中，请稍候。';
  if (status.connected) return '连接验证通过。';
  return status.connectionNotice || '当前登录已过期或尚未连接，请重新扫码。';
}

// A panel owns one attempt. Check ownership after every asynchronous reply,
// including login_start: closing before the reservation arrives must cancel it.
export function createQqLogin(call: Call, events: Events, delay = 2000) {
  let generation = 0, attemptId: number | undefined, disposed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cancelRemote = (id: number) => call('cancel_login', { attemptId: id }).catch(() => {});
  const cancel = () => {
    ++generation; clearTimeout(timer); timer = undefined;
    if (attemptId !== undefined) void cancelRemote(attemptId);
    attemptId = undefined;
  };
  const start = async () => {
    if (disposed) return;
    cancel(); const own = generation;
    const current = () => !disposed && own === generation;
    try {
      const reserved = await call('login_start');
      const id = reserved.attemptId;
      if (id === undefined) throw new Error('无法创建登录请求，请重试');
      if (!current()) { void cancelRemote(id); return; }
      attemptId = id;
      const qr = await call('qr', { attemptId: id });
      if (!current()) return;
      if (!qr.image) throw new Error('二维码未返回，请重试');
      events.image(qr.image);
      events.message('请用手机 QQ 扫码，并在手机上确认。关闭面板会取消本次扫码。');
      const poll = async () => {
        if (!current()) return;
        try {
          const result = await call('poll', { attemptId: id });
          if (!current()) return;
          events.message(result.message || '正在等待确认…');
          if (result.connected) { attemptId = undefined; events.connected(); return; }
          if (result.expired || result.cancelled) { cancel(); return; }
          timer = setTimeout(() => void poll(), delay);
        } catch (error) {
          if (!current()) return;
          cancel(); events.message(`${(error as Error).message}。可重新获取二维码。`);
        }
      };
      void poll();
    } catch (error) {
      if (!current()) return;
      cancel(); events.message(`${(error as Error).message}。可重新获取二维码。`);
    }
  };
  return { start, cancel, dispose: () => { disposed = true; cancel(); } };
}
