import { Suspense } from "react";

import { EpicDetailView } from "./epic-detail-view";

function LoadingFallback() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-[#0a0a0a] text-zinc-400">
      Loading epic…
    </div>
  );
}

export default function EpicPage() {
  return (
    <Suspense fallback={<LoadingFallback />}>
      <EpicDetailView />
    </Suspense>
  );
}
