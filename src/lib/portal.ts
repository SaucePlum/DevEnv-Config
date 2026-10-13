// 把节点传送到 body 下渲染：
// 卡片上的 backdrop-filter 会为 fixed 后代创建新的定位基准（containin block），
// 使弹窗被困在卡片内；传送到 body 后弹窗真正相对视口全屏。
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
