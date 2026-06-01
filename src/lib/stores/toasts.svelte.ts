// Minimal toast store (Svelte 5 runes). Used for webhook + save feedback.

export type ToastKind = "success" | "error" | "info";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

let nextId = 0;

class ToastStore {
  items = $state<Toast[]>([]);

  push(message: string, kind: ToastKind = "info", ttl = 3500) {
    const id = nextId++;
    this.items = [...this.items, { id, kind, message }];
    setTimeout(() => this.dismiss(id), ttl);
  }

  success(message: string) {
    this.push(message, "success");
  }

  error(message: string) {
    this.push(message, "error", 6000);
  }

  dismiss(id: number) {
    this.items = this.items.filter((t) => t.id !== id);
  }
}

export const toasts = new ToastStore();
