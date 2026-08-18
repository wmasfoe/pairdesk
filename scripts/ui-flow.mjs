/**
 * M1 UI 交互自测：真实浏览器驱动。
 *
 * 流程：首页 → 点"控制端"卡片 → 填密码 → 点"连接" → 等 mock 推帧 → 截图。
 * 用 Playwright 无头浏览器（已装 chromium），验证页面可交互、画面能渲染。
 */
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const OUT = '/tmp/pd-ui';
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1100, height: 760 } });

// 1) 首页
await page.goto('http://127.0.0.1:4173');
await page.waitForTimeout(700);
await page.screenshot({ path: `${OUT}/1-home.png` });
console.log('✅ 首页已截图');

// 2) 进入控制端页
await page.locator('.pd-modecard').nth(1).click();
await page.waitForTimeout(500);
await page.screenshot({ path: `${OUT}/2-viewer-form.png` });
console.log('✅ 已进入控制端页(表单)');

// 3) 填密码 + 点连接（地址默认 127.0.0.1:8888）
await page.getByPlaceholder('一次性密码').fill('123456');
await page.getByRole('button', { name: '连接' }).click();
await page.waitForTimeout(1800); // mock 推帧
await page.screenshot({ path: `${OUT}/3-viewer-connected.png` });
console.log('✅ 已连接并等待画面');

// 4) 断言画面区出现了 <img>（mock 帧已渲染）
const imgCount = await page.locator('.pd-video img').count();
const hasImg = imgCount > 0;
console.log(hasImg ? '✅ 画面已渲染 (img 出现)' : '❌ 未检测到画面');
await page.screenshot({ path: `${OUT}/4-final.png` });

// 5) 验证被控端页也能进
await page.getByRole('button', { name: '断开' }).click();
await page.waitForTimeout(300);
await page.getByRole('button', { name: '← 返回' }).click();
await page.waitForTimeout(300);
await page.locator('.pd-modecard').nth(0).click();
await page.waitForTimeout(300);
await page.screenshot({ path: `${OUT}/5-host.png` });
console.log('✅ 被控端页已进入');

await browser.close();
if (!hasImg) process.exit(1);
console.log('🎉 UI 交互自测通过');
