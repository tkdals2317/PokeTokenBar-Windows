// Floating pet window and its hover callout window (pet-settings-misc.md §2 #1-13, §3.1, §5.3).
// Owner: pet-settings-misc area. Geometry (bubble headroom, run to cursor, persisted origin) is Rust's.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Menu } from "@tauri-apps/api/menu";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LangContext, pick } from "../../kit/i18n";
import { CompanionEvent, EGG, Settings, displayed, nameOf, todayTotal, useBackend } from "../../shared";
import {
  AgentSummary, LimitAlert, PettingDetector, bubbleContent, capFrameRate, frameFloor, heartTop, hoverText,
  panelSize, pickBubble, prayerEmojiSize, strokeDistance,
} from "./petLogic";
import { petStrings } from "./strings";
import "./pet.css";

const SPRITES = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon";
const gifUrl = (id: number, shiny: boolean, back: boolean) =>
  `${SPRITES}/versions/generation-v/black-white/animated/${back ? "back/" : ""}${shiny ? "shiny/" : ""}${id}.gif`;

const CLICK_SLOP_SQUARED = 16;
const BUBBLE_TTL = 6000;
const EVENT_POP = 6000;
const HEART_MS = 1800;
const BOB_MS = 500;
const PRAYER_MS = 1100;
const BUBBLE_CONTENT_WIDTH = 164;
const CALLOUT_MARGIN = 4; // room for the shadow; Rust leaves the matching gap

/** `pet-state` payload (pet::PetView). */
type PetView = {
  faces_back: boolean;
  sulk_idle: number | null;
  egg_prayers: number;
  motion_allowed: boolean;
  low_power: boolean;
  agent: AgentSummary;
};

export default function Pet() {
  return getCurrentWindow().label === "pet-callout" ? <Callout /> : <FloatingPet />;
}

/** Click-through callout window: shows whatever text the pet window sent. */
function Callout() {
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    invoke<string | null>("pet_callout_text").then(setText);
    const sub = listen<string | null>("pet-callout", (e) => setText(e.payload));
    return () => {
      sub.then((f) => f());
    };
  }, []);
  return text ? <div className="pet-callout" style={{ margin: CALLOUT_MARGIN }}>{text}</div> : null;
}

// ---- text measuring (bubble fitting, callout size) ----

let canvas: HTMLCanvasElement | null = null;
function textWidth(text: string, font: string) {
  canvas ??= document.createElement("canvas");
  const ctx = canvas.getContext("2d")!;
  ctx.font = font;
  return ctx.measureText(text).width;
}

/** Lines `text` wraps to at the bubble width, measured in a hidden element styled like the body. */
function bodyLines(text: string) {
  const el = document.getElementById("pet-measure-body");
  if (!el) return 1;
  el.textContent = "M";
  const line = el.getBoundingClientRect().height;
  el.textContent = text;
  return Math.round(el.getBoundingClientRect().height / line);
}

function calloutSize(text: string) {
  const el = document.getElementById("pet-measure-callout");
  if (!el) return { width: 0, height: 0 };
  el.textContent = text;
  const r = el.getBoundingClientRect();
  return { width: Math.ceil(r.width) + CALLOUT_MARGIN * 2, height: Math.ceil(r.height) + CALLOUT_MARGIN * 2 };
}

const fitsTitle = (t: string) => textWidth(t, `bold 11px ${getComputedStyle(document.body).fontFamily}`) <= BUBBLE_CONTENT_WIDTH;
const fitsBody = (_title: string, body: string) => bodyLines(body) <= 2;

// ---- sprite (GIF frames capped to the animation-quality floor) ----

type Frames = { images: ImageBitmap[]; delays: number[]; w: number; h: number; visibleTop: number };

// WebView2 (Chromium) has WebCodecs' ImageDecoder; elsewhere the <img> fallback plays the GIF as is.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const ImageDecoderCtor = (globalThis as any).ImageDecoder as any;

/** First opaque row of a frame inside the central 35–65 % band (alpha > 16). */
function opaqueTop(img: ImageBitmap): number {
  const c = document.createElement("canvas");
  c.width = img.width;
  c.height = img.height;
  const ctx = c.getContext("2d")!;
  ctx.drawImage(img, 0, 0);
  const data = ctx.getImageData(0, 0, c.width, c.height).data;
  const [x0, x1] = [Math.floor(c.width * 0.35), Math.ceil(c.width * 0.65)];
  for (let y = 0; y < c.height; y++) {
    for (let x = x0; x < x1; x++) if (data[(y * c.width + x) * 4 + 3] > 16) return y;
  }
  return 0;
}

async function decodeGif(url: string): Promise<Frames | "missing" | null> {
  const res = await fetch(url).catch(() => null);
  if (!res) return null;
  if (res.status === 404) return "missing";
  if (!res.ok || !ImageDecoderCtor) return null;
  const dec = new ImageDecoderCtor({ data: await res.arrayBuffer(), type: "image/gif" });
  await dec.tracks.ready;
  const count: number = dec.tracks.selectedTrack?.frameCount ?? 1;
  const images: ImageBitmap[] = [];
  const delays: number[] = [];
  for (let i = 0; i < count; i++) {
    const { image } = await dec.decode({ frameIndex: i });
    images.push(await createImageBitmap(image));
    const d = (image.duration ?? 0) / 1e6;
    delays.push(d < 0.02 ? 0.1 : d);
    image.close();
  }
  dec.close();
  return { images, delays, w: images[0].width, h: images[0].height, visibleTop: opaqueTop(images[Math.floor(images.length / 2)]) };
}

function Sprite(props: {
  id: number | null; shiny: boolean; back: boolean; size: number; animate: boolean; floor: number;
  onVisibleTop: (top: number) => void; label: string;
}) {
  const { id, shiny, back, size, animate, floor, onVisibleTop } = props;
  const ref = useRef<HTMLCanvasElement>(null);
  const [frames, setFrames] = useState<Frames | null>(null);
  const [fallback, setFallback] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setFrames(null);
    setFallback(null);
    if (id == null) {
      setFallback(EGG);
      return;
    }
    (async () => {
      let f = await decodeGif(gifUrl(id, shiny, back));
      if (f === "missing" && back) f = await decodeGif(gifUrl(id, shiny, false)); // no back sprite: front
      if (!live) return;
      if (f && f !== "missing") setFrames(f);
      else setFallback(gifUrl(id, shiny, back && f !== "missing"));
    })();
    return () => {
      live = false;
    };
  }, [id, shiny, back]);

  const capped = useMemo(() => {
    if (!frames) return [];
    const list = frames.images.map((image, i) => ({ image, delay: frames.delays[i] }));
    return animate ? capFrameRate(list, floor) : list.slice(0, 1);
  }, [frames, animate, floor]);

  useEffect(() => {
    if (!frames) return;
    const s = Math.min(size / frames.w, size / frames.h);
    onVisibleTop(size - frames.h * s + frames.visibleTop * s);
  }, [frames, size, onVisibleTop]);

  useEffect(() => {
    const c = ref.current;
    if (!c || !frames || capped.length === 0) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(size * dpr);
    c.height = Math.round(size * dpr);
    const ctx = c.getContext("2d")!;
    ctx.imageSmoothingEnabled = false;
    const s = Math.min(size / frames.w, size / frames.h) * dpr;
    const [dw, dh] = [frames.w * s, frames.h * s];
    let i = 0;
    let timer = 0;
    const draw = () => {
      ctx.clearRect(0, 0, c.width, c.height);
      ctx.drawImage(capped[i].image, (c.width - dw) / 2, c.height - dh, dw, dh);
      if (capped.length > 1) {
        timer = window.setTimeout(draw, capped[i].delay * 1000);
        i = (i + 1) % capped.length;
      }
    };
    draw();
    return () => window.clearTimeout(timer);
  }, [capped, frames, size]);

  if (fallback) {
    return (
      <img
        className="pet-sprite"
        src={fallback}
        alt={props.label}
        width={size}
        height={size}
        draggable={false}
        onError={(e) => id != null && back && (e.currentTarget.src = gifUrl(id, shiny, false))}
      />
    );
  }
  return <canvas ref={ref} className="pet-sprite" style={{ width: size, height: size }} aria-label={props.label} />;
}

// ---- pieces ----

const HEART_ROWS = [
  "..OOOOOOOOOOOOO..",
  ".OWWWWWWWWWWWWWO.",
  "OWWWWWWWWWWWWWWWO",
  "OWWWWRRRWRRRWWWWO",
  "OWWWRHHRRRRRRWWWO",
  "OWWWRHRRRRRRDWWWO",
  "OWWWRRRRRRRRDWWWO",
  "OWWWWRRRRRRDWWWWO",
  "OWWWWWRRRRDWWWWWO",
  "OWWWWWWRRDWWWWWWO",
  "OWWWWWWWDWWWWWWWO",
  "OWWWWWWWWWWWWWWWO",
  ".OWWWWWWWWWWWWWO.",
  "..OOOOOWWWOOOOO..",
  ".......OWO.......",
  "........O........",
];
// Game-style pixel colours, the same in light and dark mode.
const HEART_COLORS: Record<string, string> = { O: "#202020", W: "#FFFFFF", R: "#E0303C", D: "#A8202C", H: "#FF9AA2" };

function HeartBubble({ top, label }: { top: number; label: string }) {
  return (
    <svg className="pet-heart" style={{ top }} width={34} height={32} viewBox="0 0 34 32" shapeRendering="crispEdges" role="img" aria-label={label}>
      {HEART_ROWS.flatMap((row, y) =>
        Array.from(row).map((cell, x) =>
          HEART_COLORS[cell] ? <rect key={`${x}-${y}`} x={x * 2} y={y * 2} width={2} height={2} fill={HEART_COLORS[cell]} /> : null,
        ),
      )}
    </svg>
  );
}

function SpeechBubble({ title, body, critical }: { title: string; body: string; critical: boolean }) {
  return (
    <div className="pet-bubble">
      <div className={critical ? "pet-bubble-title critical" : "pet-bubble-title"}>{title}</div>
      <div className="pet-bubble-body">{body}</div>
      <svg className="pet-bubble-tail" width={10} height={6} viewBox="0 0 10 6">
        <path d="M0 0 L10 0 L5 6 Z" />
      </svg>
    </div>
  );
}

// ---- the pet ----

function highestLimit(limits: { windows: { used_percent: number }[] }[]): number | null {
  const all = limits.flatMap((p) => p.windows.map((w) => w.used_percent));
  return all.length ? Math.max(...all) : null;
}

function FloatingPet() {
  const { usage, companion, limits, settings } = useBackend();
  const [view, setView] = useState<PetView | null>(null);
  const [alert, setAlert] = useState<LimitAlert | null>(null);
  const [hearts, setHearts] = useState(false);
  const [hovering, setHovering] = useState(false);
  const [eventLine, setEventLine] = useState<string | null>(null);
  const [squashed, setSquashed] = useState(false);
  const [prayers, setPrayers] = useState<{ id: number; x: number }[]>([]);
  const [wobble, setWobble] = useState(0);
  const [visibleTop, setVisibleTop] = useState(0);
  const timers = useRef<Record<string, number>>({});
  const press = useRef<{ x: number; y: number } | null>(null);
  const detector = useRef(new PettingDetector(10));
  const lang = settings?.language ?? "en";
  const l = pick(petStrings, lang);

  const restart = (name: string, ms: number, fn: () => void) => {
    window.clearTimeout(timers.current[name]);
    timers.current[name] = window.setTimeout(fn, ms);
  };

  useEffect(() => {
    invoke<PetView>("pet_state").then(setView);
    const subs = [
      listen<PetView>("pet-state", (e) => setView(e.payload)),
      listen<LimitAlert>("pet-limit-alert", (e) => {
        setAlert(e.payload);
        restart("alert", BUBBLE_TTL, () => setAlert(null));
      }),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);

  const shown = companion ? displayed(companion, settings) : null;
  const isEgg = shown == null;
  const size = settings?.floating_pet_size ?? 96;
  const agent = view?.agent ?? null;
  const bubble = settings?.floating_pet_agent_activity === false ? pickBubble(alert, null) : pickBubble(alert, agent);
  const heartsShowing = hearts && bubble == null;
  const showingBubble = bubble != null || heartsShowing;
  const content = useMemo(() => (bubble ? bubbleContent(bubble, l, fitsTitle, fitsBody) : null), [bubble, l]);
  const name = shown ? nameOf(shown.names, shown.id, lang) : "🥚";
  detector.current.minDistance = strokeDistance(size);

  useEffect(() => {
    // The line comes localized from Rust: this closure's `companion` can predate the hatch.
    const sub = listen<CompanionEvent>("companion-event", (e) => {
      if (showingBubble) return; // not queued: an event that cannot pop is dropped
      setEventLine(e.payload.text);
      restart("event", EVENT_POP, () => setEventLine(null));
    });
    return () => {
      sub.then((f) => f());
    };
  }, [showingBubble]);

  // The panel grows upward while a bubble or hearts show (Rust keeps the pet in place).
  useEffect(() => {
    invoke("pet_layout", { showingBubble });
  }, [showingBubble]);

  // Agent "working": two-frame squash every 0.5 s, only when motion is allowed.
  const bobs = !!agent?.working && !!view?.motion_allowed;
  useEffect(() => {
    if (!bobs) return setSquashed(false);
    const id = window.setInterval(() => setSquashed((s) => !s), BOB_MS);
    return () => window.clearInterval(id);
  }, [bobs]);

  const hover = settings
    ? hoverText(l, {
        todayTokens: usage ? todayTotal(usage) : 0,
        limitUtilization: highestLimit(limits),
        mode: settings.limit_display,
        sulkIdle: view?.sulk_idle ?? null,
        eventLine,
        prayers: isEgg ? (view?.egg_prayers ?? 0) : 0,
      })
    : "";
  // Speech and hearts outrank the hover/event callout.
  const callout = !showingBubble && (hovering || eventLine) ? hover : null;
  useEffect(() => {
    const s = callout ? calloutSize(callout) : { width: 0, height: 0 };
    invoke("pet_callout", { text: callout, ...s });
  }, [callout]);

  const onVisibleTop = useCallback((t: number) => setVisibleTop(t), []);

  if (!settings || !companion) return null;
  const save = (next: Settings) => invoke("set_settings", { value: next });

  async function showMenu(e: React.MouseEvent) {
    e.preventDefault();
    const s = settings!;
    const menu = await Menu.new({
      items: [
        { id: "open", text: l.floatingPetMenuOpen, action: () => invoke("show_popover") },
        { id: "hide", text: l.floatingPetMenuHide, action: () => save({ ...s, floating_pet_enabled: false }) },
        { id: "back", text: l.backViewLabel, checked: s.always_faces_back, action: () => save({ ...s, always_faces_back: !s.always_faces_back }) },
      ],
    });
    await menu.popup();
  }

  async function click() {
    const count = await invoke<number>("pet_click", { pray: isEgg });
    if (!isEgg) return invoke("show_popover");
    const id = count + Math.random();
    setPrayers((p) => [...p, { id, x: Math.random() }]);
    setWobble((w) => w + 1);
    window.setTimeout(() => setPrayers((p) => p.filter((q) => q.id !== id)), PRAYER_MS);
  }

  const panel = panelSize(size, showingBubble);
  const emoji = prayerEmojiSize(size);
  return (
    <LangContext.Provider value={lang}>
      <div
        className="pet-root"
        style={{ width: panel.w, height: panel.h }}
        onContextMenu={showMenu}
        onPointerEnter={() => setHovering(true)}
        onPointerLeave={() => {
          setHovering(false);
          detector.current.reset();
        }}
      >
        {bubble && content && <SpeechBubble {...content} />}
        <div
          className={squashed ? "pet-body squashed" : "pet-body"}
          style={{ width: size, height: size }}
          title={heartsShowing ? undefined : hover}
          onPointerDown={(e) => {
            detector.current.reset();
            if (e.button === 0) press.current = { x: e.screenX, y: e.screenY };
          }}
          onPointerMove={(e) => {
            const p = press.current;
            if (e.buttons === 0) {
              // Stroking: hover moves with no button pressed.
              if (detector.current.feed(e.screenX, e.timeStamp / 1000) && pickBubble(alert, agent) == null) {
                setHearts(true);
                restart("hearts", HEART_MS, () => setHearts(false));
              }
              return;
            }
            if (p && (e.screenX - p.x) ** 2 + (e.screenY - p.y) ** 2 >= CLICK_SLOP_SQUARED) {
              press.current = null;
              detector.current.reset();
              getCurrentWindow().startDragging();
            }
          }}
          onPointerUp={(e) => {
            if (e.button === 0 && press.current) click();
            press.current = null;
          }}
        >
          <div key={wobble} className={wobble ? "pet-wobble" : undefined}>
            <Sprite
              id={shown?.id ?? null}
              shiny={shown?.shiny ?? false}
              back={!isEgg && !!view?.faces_back}
              size={size}
              animate={!view?.low_power}
              floor={frameFloor(settings.animation_quality, !!view?.low_power)}
              onVisibleTop={onVisibleTop}
              label={name}
            />
          </div>
          {prayers.map((p) => (
            <span
              key={p.id}
              className="pet-prayer"
              style={{
                width: emoji,
                height: emoji,
                fontSize: emoji * 0.8,
                left: p.x * (size - emoji),
                ["--from" as string]: `${size * 0.7 - emoji / 2}px`,
              }}
            >
              🙏
            </span>
          ))}
          {heartsShowing && <HeartBubble top={heartTop(visibleTop || size * 0.1)} label={l.floatingPetHeartsLabel} />}
        </div>
      </div>
      <div className="pet-measure">
        <div id="pet-measure-body" className="pet-bubble-body" style={{ width: BUBBLE_CONTENT_WIDTH }} />
        <div id="pet-measure-callout" className="pet-callout" />
      </div>
    </LangContext.Provider>
  );
}
