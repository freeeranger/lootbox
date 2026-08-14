import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as THREE from "three";
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

export function resetModelPreviewCache() {
  previews.clear();
  completedPreviews.clear();
  savedThumbnails.clear();
  persistedThumbnails.clear();
}

async function renderPreview(path: string) {
  const renderer = new THREE.WebGLRenderer({
    antialias: true,
    alpha: false,
    preserveDrawingBuffer: true,
  });
  renderer.setPixelRatio(1);
  renderer.setSize(384, 288, false);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.05;

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x17181b);
  scene.add(new THREE.HemisphereLight(0xffffff, 0x30343b, 2.6));
  const key = new THREE.DirectionalLight(0xffffff, 3.5);
  key.position.set(4, 6, 3);
  scene.add(key);
  const rim = new THREE.DirectionalLight(0xc99a45, 1.35);
  rim.position.set(-4, 2, -4);
  scene.add(rim);

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
    const box = new THREE.Box3().setFromObject(model);
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    if (box.isEmpty() || ![size.x, size.y, size.z, center.x, center.y, center.z].every(Number.isFinite)) {
      throw new Error("The model has invalid preview bounds");
    }
    const radius = Math.max(size.x, size.y, size.z) || 1;
    model.position.sub(center);

    const near = Math.max(radius / 100, 0.001);
    const camera = new THREE.PerspectiveCamera(36, 4 / 3, near, Math.max(radius * 20, near + 1));
    camera.position.set(radius * 1.55, radius * 1.1, radius * 2.05);
    camera.lookAt(0, 0, 0);
    renderer.render(scene, camera);
    const source = renderer.domElement.toDataURL("image/png");
    if (!source.startsWith("data:image/png;base64,") || source.length < 256) {
      throw new Error("The model renderer returned an empty thumbnail");
    }
    return source;
  } finally {
    if (model) disposeModel(model);
    renderer.dispose();
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
  const [source, setSource] = useState<string | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let active = true;
    setSource(null);
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        void prepareModelThumbnail(asset)
          .then((nextSource) => {
            if (!nextSource) return;
            if (active) setSource(nextSource);
          })
          .catch((error) => onError?.(error));
      },
      { rootMargin: "240px" },
    );
    observer.observe(host);
    return () => {
      active = false;
      observer.disconnect();
    };
  }, [asset.absolutePath, asset.id, onError]);

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
        <AssetTypeIcon type="model" size={iconSize} />
      )}
    </span>
  );
}
