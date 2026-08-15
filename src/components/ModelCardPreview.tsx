import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as THREE from "three";
import { LoaderCircle } from "lucide-react";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { api } from "../api";
import type { Asset } from "../types";
import { AssetTypeIcon } from "./AssetTypeIcon";
import { disposeModel, prepareModelForPreview } from "./modelPreviewUtils";

const previews = new Map<string, Promise<string>>();
const completedPreviews = new Set<string>();
const savedThumbnails = new Set<number>();
const persistedThumbnails = new Map<number, string>();
const previewCacheLimit = 48;
const previewTimeoutMs = 20_000;
let renderQueue: Promise<void> = Promise.resolve();
let sharedRenderer: THREE.WebGLRenderer | null = null;
let sharedScene: THREE.Scene | null = null;

function getSharedRendererScene() {
  if (sharedRenderer && sharedRenderer.getContext().isContextLost()) {
    try {
      sharedRenderer.dispose();
    } catch {
      // ignore disposal errors on lost context
    }
    sharedRenderer = null;
    sharedScene = null;
  }

  if (!sharedRenderer) {
    const canvas = document.createElement("canvas");
    canvas.addEventListener("webglcontextlost", (event) => {
      event.preventDefault();
      try {
        sharedRenderer?.dispose();
      } catch {
        // ignore
      }
      sharedRenderer = null;
      sharedScene = null;
    });

    sharedRenderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      alpha: true,
      preserveDrawingBuffer: true,
      powerPreference: "high-performance",
    });
    sharedRenderer.setPixelRatio(1);
    sharedRenderer.setSize(256, 192, false);
    sharedRenderer.outputColorSpace = THREE.SRGBColorSpace;
    sharedRenderer.toneMapping = THREE.ACESFilmicToneMapping;
    sharedRenderer.toneMappingExposure = 1.05;
    sharedRenderer.setClearColor(0x17181b, 1);

    sharedScene = new THREE.Scene();
    sharedScene.background = new THREE.Color(0x17181b);
    sharedScene.add(new THREE.HemisphereLight(0xffffff, 0x30343b, 2.6));
    const key = new THREE.DirectionalLight(0xffffff, 3.5);
    key.position.set(4, 6, 3);
    sharedScene.add(key);
    sharedScene.add(key.target);
    const rim = new THREE.DirectionalLight(0xc99a45, 1.35);
    rim.position.set(-4, 2, -4);
    sharedScene.add(rim);
    sharedScene.add(rim.target);
  }
  return { renderer: sharedRenderer, scene: sharedScene! };
}

export function resetModelPreviewCache() {
  previews.clear();
  completedPreviews.clear();
  savedThumbnails.clear();
  persistedThumbnails.clear();
  if (sharedRenderer) {
    try {
      sharedRenderer.dispose();
    } catch {
      // ignore
    }
    sharedRenderer = null;
    sharedScene = null;
  }
}

async function renderPreview(path: string, retryOnContextLoss = true): Promise<string> {
  const { renderer, scene } = getSharedRendererScene();
  const gl = renderer.getContext();
  if (gl.isContextLost()) {
    try {
      sharedRenderer?.dispose();
    } catch {
      // ignore
    }
    sharedRenderer = null;
    sharedScene = null;
    if (retryOnContextLoss) {
      return renderPreview(path, false);
    }
    throw new Error("WebGL context is lost");
  }

  let model: THREE.Object3D | null = null;
  try {
    const gltf = await new GLTFLoader().loadAsync(convertFileSrc(path));
    model = gltf.scene;
    prepareModelForPreview(model);
    scene.add(model);
    let renderableMeshes = 0;
    model.traverse((child) => {
      if (child instanceof THREE.Mesh && child.visible && child.geometry.getAttribute("position")?.count > 0) renderableMeshes += 1;
    });
    if (renderableMeshes === 0) throw new Error("The model contains no renderable geometry");
    model.updateMatrixWorld(true);
    const box = new THREE.Box3().setFromObject(model);
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    if (box.isEmpty() || ![size.x, size.y, size.z, center.x, center.y, center.z].every(Number.isFinite)) {
      throw new Error("The model has invalid preview bounds");
    }
    const radius = Math.max(size.x, size.y, size.z) || 1;
    model.position.sub(center);
    model.updateMatrixWorld(true);

    const near = Math.max(radius / 100, 0.001);
    const far = Math.max(radius * 20, near + 1);
    const camera = new THREE.PerspectiveCamera(36, 4 / 3, near, far);
    camera.position.set(radius * 1.55, radius * 1.1, radius * 2.05);
    camera.lookAt(0, 0, 0);
    camera.updateMatrixWorld();
    camera.updateProjectionMatrix();

    if (gltf.animations.length > 0) {
      const mixer = new THREE.AnimationMixer(model);
      mixer.clipAction(gltf.animations[0]).play();
      mixer.update(0.01);
    }

    renderer.setClearColor(0x17181b, 1);
    renderer.compile(scene, camera);
    renderer.render(scene, camera);

    if (gl.isContextLost()) {
      try {
        sharedRenderer?.dispose();
      } catch {
        // ignore
      }
      sharedRenderer = null;
      sharedScene = null;
      if (retryOnContextLoss) {
        return renderPreview(path, false);
      }
      throw new Error("WebGL context lost while rendering model thumbnail");
    }

    // Verify rendered pixels: alpha should be 255 because alpha: true and setClearColor(0x17181b, 1) is set
    const cornerSample = new Uint8Array(4);
    gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, cornerSample);
    if (cornerSample[3] === 0) {
      try {
        sharedRenderer?.dispose();
      } catch {
        // ignore
      }
      sharedRenderer = null;
      sharedScene = null;
      if (retryOnContextLoss) {
        return renderPreview(path, false);
      }
      throw new Error("The model renderer returned an unrendered frame");
    }

    const source = renderer.domElement.toDataURL("image/png");
    if (!source.startsWith("data:image/png;base64,") || source.length < 1500) {
      throw new Error("The model renderer returned an empty thumbnail");
    }
    return source;
  } catch (error) {
    if (retryOnContextLoss && (gl.isContextLost() || (error instanceof Error && error.message.toLowerCase().includes("webgl")))) {
      try {
        sharedRenderer?.dispose();
      } catch {
        // ignore
      }
      sharedRenderer = null;
      sharedScene = null;
      return renderPreview(path, false);
    }
    throw error;
  } finally {
    if (model) {
      scene.remove(model);
      disposeModel(model);
    }
  }
}

function renderPreviewWithTimeout(path: string) {
  return new Promise<string>((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error("Model thumbnail generation timed out")), previewTimeoutMs);
    void renderPreview(path).then(
      (source) => { window.clearTimeout(timer); resolve(source); },
      (error) => { window.clearTimeout(timer); reject(error); },
    );
  });
}

function getPreview(path: string) {
  const existing = previews.get(path);
  if (existing) {
    previews.delete(path);
    previews.set(path, existing);
    return existing;
  }

  let resolvePreview: (source: string) => void = () => {};
  let rejectPreview: (error: unknown) => void = () => {};
  const preview = new Promise<string>((resolve, reject) => {
    resolvePreview = resolve;
    rejectPreview = reject;
  });
  previews.set(path, preview);
  renderQueue = renderQueue
    .then(async () => {
      await new Promise<void>((resolve) => {
        if (typeof window.requestIdleCallback === "function") {
          window.requestIdleCallback(() => resolve(), { timeout: 32 });
        } else {
          window.setTimeout(resolve, 16);
        }
      });
      try {
        resolvePreview(await renderPreviewWithTimeout(path));
        completedPreviews.add(path);
        const completed = previews.get(path);
        if (completed) {
          previews.delete(path);
          previews.set(path, completed);
        }
        while (completedPreviews.size > previewCacheLimit) {
          let oldest: string | undefined;
          for (const key of previews.keys()) {
            if (completedPreviews.has(key)) {
              oldest = key;
              break;
            }
          }
          if (!oldest) break;
          previews.delete(oldest);
          completedPreviews.delete(oldest);
        }
      } catch (error) {
        previews.delete(path);
        completedPreviews.delete(path);
        rejectPreview(error);
      }
    })
    .catch(() => {});
  return preview;
}

export async function prepareModelThumbnail(asset: Asset) {
  if (
    asset.thumbnailPath ||
    asset.assetType !== "model" ||
    !["glb", "gltf"].includes(asset.extension)
  ) {
    return null;
  }
  const persisted = persistedThumbnails.get(asset.id);
  if (persisted) return convertFileSrc(persisted);

  const source = await getPreview(asset.absolutePath);
  if (source && !savedThumbnails.has(asset.id)) {
    savedThumbnails.add(asset.id);
    try {
      const thumbnailPath = await api.saveModelThumbnail(
        asset.id,
        source.slice(source.indexOf(",") + 1),
      );
      persistedThumbnails.set(asset.id, thumbnailPath);
    } catch (error) {
      savedThumbnails.delete(asset.id);
      throw error;
    }
  }
  return source;
}

export function ModelCardPreview({ asset, iconSize, onError }: { asset: Asset; iconSize: number; onError?: (error: unknown) => void }) {
  const hostRef = useRef<HTMLSpanElement>(null);
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;

  const [source, setSource] = useState<string | null>(() => {
    const persisted = persistedThumbnails.get(asset.id);
    return persisted ? convertFileSrc(persisted) : null;
  });

  useEffect(() => {
    const persisted = persistedThumbnails.get(asset.id);
    if (persisted) {
      setSource(convertFileSrc(persisted));
      return;
    }
    const host = hostRef.current;
    if (!host) return;
    let active = true;
    let timeoutId: number | null = null;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        timeoutId = window.setTimeout(() => {
          if (!active) return;
          void prepareModelThumbnail(asset)
            .then((nextSource) => {
              if (!nextSource) return;
              if (active) setSource(nextSource);
            })
            .catch((error) => onErrorRef.current?.(error));
        }, 60);
      },
      { rootMargin: "160px" },
    );
    observer.observe(host);
    return () => {
      active = false;
      if (timeoutId !== null) window.clearTimeout(timeoutId);
      observer.disconnect();
    };
  }, [asset.absolutePath, asset.id]);

  return (
    <span ref={hostRef} className="grid size-full place-items-center text-muted-foreground/65">
      {source ? (
        <img src={source} alt="" className="size-full object-contain" onError={() => {
          persistedThumbnails.delete(asset.id);
          savedThumbnails.delete(asset.id);
          setSource(null);
          onError?.(new Error(`Generated model thumbnail could not be displayed for ${asset.relativePath}`));
        }} />
      ) : (
        <span className="relative grid size-full place-items-center">
          <AssetTypeIcon type="model" size={iconSize} />
          <LoaderCircle className="absolute right-2 bottom-2 size-3 animate-spin text-primary/70" aria-hidden="true" />
          <span className="sr-only">Preparing model preview</span>
        </span>
      )}
    </span>
  );
}
