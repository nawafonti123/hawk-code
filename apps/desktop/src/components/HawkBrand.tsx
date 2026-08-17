interface HawkBrandProps {
  compact?: boolean;
}

export function HawkBrand({ compact = false }: HawkBrandProps) {
  return (
    <div className="hawk-brand" aria-label="HAWK Code">
      <span className="hawk-brand__mark">
        <img src="/brand/hawk-code-mark.png" alt="" />
      </span>
      {!compact && (
        <span className="hawk-brand__wordmark">
          <strong>HAWK</strong>
          <span>Code</span>
        </span>
      )}
    </div>
  );
}
