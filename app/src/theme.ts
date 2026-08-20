/**
 * 跟随操作系统明暗外观。
 * 在 <html> 上切换 .pd-light / .pd-dark（以及 Tailwind 的 .dark），
 * ui-kit 与页面 token 都从这套类名取色。
 */
const QUERY = '(prefers-color-scheme: dark)';

function apply(dark: boolean) {
  const root = document.documentElement;
  root.classList.toggle('dark', dark);
  root.classList.toggle('pd-dark', dark);
  root.classList.toggle('pd-light', !dark);
}

export function watchSystemTheme() {
  const mq = window.matchMedia(QUERY);
  apply(mq.matches);
  mq.addEventListener('change', (e) => apply(e.matches));
}
