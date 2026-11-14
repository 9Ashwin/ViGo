import { useEffect, useRef } from 'react';
import { SpriteMascot } from '../../shared/mascot';
export function MascotView() {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const svg = ref.current;
    if (!svg) return;
    const mascot = new SpriteMascot(svg, 76, 80);
    let visible = false;
    const observer = new IntersectionObserver(([entry]) => { visible = entry.isIntersecting; if (visible && !document.hidden) mascot.start(); else mascot.stop(); });
    observer.observe(svg);
    const point = (event: PointerEvent) => { const b = svg.getBoundingClientRect(); const dx = event.clientX - b.x - b.width / 2, dy = event.clientY - b.y - b.height / 2; mascot.point(dx, dy, Math.hypot(dx, dy) < 52); };
    const visibility = () => { if (document.hidden) mascot.stop(); else if (visible) mascot.start(); };
    document.addEventListener('pointermove', point); document.addEventListener('visibilitychange', visibility);
    return () => { observer.disconnect(); mascot.stop(); document.removeEventListener('pointermove', point); document.removeEventListener('visibilitychange', visibility); };
  }, []);
  return <canvas ref={ref} className="mascotSprite" aria-label="ViGo 小猫" />;
}
