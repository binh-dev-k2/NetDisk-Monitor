import type { ReactNode } from 'react';
import { sparklinePoints } from '../lib/sparkline';
import { formatRate } from '../lib/format';

function Sparkline({ values, tone }: { values: number[]; tone: string }) {
  const points = sparklinePoints(values, 200, 36);
  if (!points) return null;
  const fillPoints = `${points} 200,36 0,36`;
  return (
    <svg className={`sparkline ${tone}`} viewBox="0 0 200 36" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient id={`grad-${tone}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="currentColor" stopOpacity="0.22" />
          <stop offset="100%" stopColor="currentColor" stopOpacity="0.00" />
        </linearGradient>
      </defs>
      <polygon points={fillPoints} fill={`url(#grad-${tone})`} style={{ stroke: 'none' }} />
      <polyline points={points} />
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
  return (
    <section className={`metric-card ${tone}`}>
      <div className="metric-card-top">
        <div className="metric-card-title">
          <span className="metric-icon">{icon}</span>
          <span>{label}</span>
        </div>
        <span className="metric-card-peak">
          Peak: <b>{formatRate(peak)}</b>
        </span>
      </div>
      <div className="metric-card-body">
        <strong>{value}</strong>
      </div>
      <Sparkline values={history} tone={tone} />
    </section>
  );
}
