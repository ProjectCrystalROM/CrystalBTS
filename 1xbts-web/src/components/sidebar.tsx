"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useSyncExternalStore } from "react";
import { useConnectionState } from "@/lib/use-event-stream";
import { useBaseStations } from "@/lib/use-base-stations";

const links = [
  { href: "/", label: "Dashboard", icon: "✦" },
  { href: "/bts", label: "Cells", icon: "◌" },
  { href: "/base-stations", label: "Base stations", icon: "⌁" },
  { href: "/messages", label: "Radio chatter", icon: "〰" },
  { href: "/mobiles", label: "Pocket pals", icon: "♡" },
  { href: "/subscribers", label: "Kittens", icon: "⊹" },
  { href: "/prls", label: "Wander maps", icon: "◆" },
  { href: "/smsc", label: "Texts & MMS", icon: "✉" },
  { href: "/packets", label: "Packet data", icon: "↓" },
  { href: "/evdo", label: "EV-DO", icon: "⚡" },
];

type Theme = "dark" | "light";

const THEME_CHANGE_EVENT = "cdma-web-theme-change";

function getThemeSnapshot(): Theme {
  if (typeof window === "undefined") {
    return "dark";
  }
  return localStorage.getItem("theme") === "light" ? "light" : "dark";
}

function subscribeThemeChange(onStoreChange: () => void) {
  window.addEventListener("storage", onStoreChange);
  window.addEventListener(THEME_CHANGE_EVENT, onStoreChange);
  return () => {
    window.removeEventListener("storage", onStoreChange);
    window.removeEventListener(THEME_CHANGE_EVENT, onStoreChange);
  };
}

function useSidebarTheme() {
  return useSyncExternalStore(subscribeThemeChange, getThemeSnapshot, () => "dark");
}

function ThemeToggle() {
  const theme = useSidebarTheme();

  useEffect(() => {
    document.documentElement.classList.toggle("light", theme === "light");
  }, [theme]);

  const toggle = () => {
    const next = theme === "dark" ? "light" : "dark";
    localStorage.setItem("theme", next);
    document.documentElement.classList.toggle("light", next === "light");
    window.dispatchEvent(new Event(THEME_CHANGE_EVENT));
  };

  return (
    <button
      onClick={toggle}
      className="flex items-center gap-2 w-full px-3 py-2 rounded-[14px_10px_14px_11px] text-[13px] font-medium text-muted hover:text-primary hover:bg-hover transition-colors"
      title={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
    >
      {theme === "dark" ? (
        <svg className="w-4 h-4" viewBox="0 0 20 20" fill="currentColor">
          <path fillRule="evenodd" d="M10 2a1 1 0 011 1v1a1 1 0 11-2 0V3a1 1 0 011-1zm4 8a4 4 0 11-8 0 4 4 0 018 0zm-.464 4.95l.707.707a1 1 0 001.414-1.414l-.707-.707a1 1 0 00-1.414 1.414zm2.12-10.607a1 1 0 010 1.414l-.706.707a1 1 0 11-1.414-1.414l.707-.707a1 1 0 011.414 0zM17 11a1 1 0 100-2h-1a1 1 0 100 2h1zm-7 4a1 1 0 011 1v1a1 1 0 11-2 0v-1a1 1 0 011-1zM5.05 6.464A1 1 0 106.465 5.05l-.708-.707a1 1 0 00-1.414 1.414l.707.707zm1.414 8.486l-.707.707a1 1 0 01-1.414-1.414l.707-.707a1 1 0 011.414 1.414zM4 11a1 1 0 100-2H3a1 1 0 000 2h1z" clipRule="evenodd" />
        </svg>
      ) : (
        <svg className="w-4 h-4" viewBox="0 0 20 20" fill="currentColor">
          <path d="M17.293 13.293A8 8 0 016.707 2.707a8.001 8.001 0 1010.586 10.586z" />
        </svg>
      )}
      {theme === "dark" ? "Light mode" : "Dark mode"}
    </button>
  );
}

function BrandLogo() {
  return (
    <div className="px-4 pt-5 pb-4">
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src="/crystalbts-sharkey.svg"
        alt="CrystalBTS"
        className="h-auto w-full max-w-[188px]"
      />
    </div>
  );
}

export function Sidebar() {
  const pathname = usePathname();
  const connection = useConnectionState();
  const online = connection === "online";
  const offline = connection === "offline";
  const { nodes } = useBaseStations();
  const cells = nodes.flatMap((node) => node.cells);
  const inService = cells.filter((cell) => cell.inService).length;

  return (
    <aside className="glass-sidebar flex flex-col h-full sticky top-0">
      {/* Brand */}
      <BrandLogo />


      {/* Nav links */}
      <nav className="flex-1 px-2 flex flex-col gap-1">
        {links.map((link) => {
          const active =
            link.href === "/"
              ? pathname === "/"
              : pathname.startsWith(link.href);
          return (
            <Link
              key={link.href}
              href={link.href}
              className={`flex items-center gap-2.5 text-[13px] font-medium px-3 py-2 rounded-[14px_10px_15px_11px] transition-colors ${
                active
                  ? "bg-active text-primary font-semibold shadow-[2px_3px_0_var(--border-primary)] -rotate-[0.3deg]"
                  : "text-muted hover:text-primary hover:bg-hover"
              }`}
            >
              <span
                aria-hidden="true"
                className={`grid h-5 w-5 shrink-0 place-items-center rounded-[8px_6px_8px_6px] text-xs leading-none transition-colors ${
                  active ? "bg-accent-indigo text-white shadow-[1px_2px_0_var(--border-primary)]" : "bg-surface-raised text-dimmed"
                }`}
              >
                {link.icon}
              </span>
              {link.label}
            </Link>
          );
        })}
      </nav>

      {/* Footer */}
      <div className="px-2 py-3 border-t-2 border-border space-y-1">
        <div className="px-2">
          <div className={`flex items-center gap-1.5 text-xs font-medium ${online ? "text-primary" : offline ? "text-accent-red" : "text-muted"}`}>
            <span
              className={`w-1.5 h-1.5 rounded-full ${
                online
                  ? "bg-live shadow-[0_0_6px_var(--live-color)]"
                  : offline
                    ? "bg-accent-red"
                    : "bg-dimmed"
              }`}
            />
            {online ? "Online" : offline ? "Offline" : "Connecting"}
          </div>
          <div className="text-[11px] text-dimmed mt-1 font-mono">
            {nodes.length > 0
              ? `${nodes.length} BS · ${inService}/${cells.length} cells in service`
              : "—"}
          </div>
        </div>
        <ThemeToggle />
      </div>
    </aside>
  );
}
