import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

/**
 * Vite 构建配置。
 *
 * 关键点：
 *  - 构建产物输出到 ./dist，供 Tauri 壳（crates/pairdesk-app）读取。
 *  - base 设为相对路径：Tauri 以自定义协议加载前端，必须相对引用资源。
 *  - clearScreen false：避免污染 Tauri 壳的终端日志。
 */
export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: './',
  clearScreen: false,
  build: {
    outDir: 'dist',
    // Tauri 在固定窗口内加载，无需代码分割造成额外请求
    rollupOptions: { output: { inlineDynamicImports: true } },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
});
