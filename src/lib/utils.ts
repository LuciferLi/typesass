import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

// 合并 shadcn 风格组件的条件类名，并解决 Tailwind 类名冲突。
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
