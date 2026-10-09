export function ErrorBanner({ message, onClose }: { message: string; onClose(): void }) {
  return (
    <div className="error-banner" role="alert">
      {message}
      <button type="button" className="btn btn--secondary error-banner__close" onClick={onClose}>
        閉じる
      </button>
    </div>
  );
}
