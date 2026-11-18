import type { ReactNode } from 'react';
import { sparklinePoints } from '../lib/sparkline';
import { formatRate } from '../lib/format';

function Sparkline({ values, tone }: { values: number[]; tone: string }) {
  const points = sparklinePoints(values, 200, 36);
  if (!points) return null;
  const fillPoints = `${points} 200,36 0,36`;
  const lastPair = points.trim().split(' ').at(-1)?.split(',');
  const [lastX, lastY] = lastPair?.length === 2 ? lastPair : [];
  return (
    <svg className={`sparkline ${tone}`} viewBox="0 0 200 36" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient id={`grad-${tone}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="currentColor" stopOpacity="0.2" />
          <stop offset="100%" stopColor="currentColor" stopOpacity="0.00" />
        </linearGradient>
      </defs>
      <line x1="0" y1="18" x2="200" y2="18" className="sparkline-guide" />
      <polygon points={fillPoints} fill={`url(#grad-${tone})`} style={{ stroke: 'none' }} />
      <polyline points={points} strokeLinecap="round" strokeLinejoin="round" />
      {lastX !== undefined && lastY !== undefined && (
        <circle cx={lastX} cy={lastY} r="2.75" className="sparkline-dot" />
      )}
    </svg>
  );
}

type MetricCardProps = {
  icon: ReactNode;
  label: string;
  value: string;
  tone: string;
  history: number[];
};

export function MetricCard({ icon, label, value, tone, history }: MetricCardProps) {
  const peak = history.length ? Math.max(...history) : 0;
  const splitAt = value.lastIndexOf(' ');
  const figure = splitAt > 0 ? value.slice(0, splitAt) : value;
  const unit = splitAt > 0 ? value.slice(splitAt) : '';
  return (
    <section className={`metric-card ${tone}`}>
      <div className="metric-card-top">
        <div className="metric-card-title">
          <span className="metric-icon">{icon}</span>
          <span>{label}</span>
        </div>
        <span className="metric-card-peak">
          Đỉnh: <b>{formatRate(peak)}</b>
        </span>
      </div>
      <div className="metric-card-body">
        <strong>{figure}{unit && <span className="metric-unit">{unit}</span>}</strong>
      </div>
      <Sparkline values={history} tone={tone} />
    </section>
  );
}
