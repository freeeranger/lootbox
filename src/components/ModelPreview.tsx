import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import type { ModelStats } from "../types";
import { disposeModel, prepareModelForPreview } from "./modelPreviewUtils";

interface Props {
  path: string;
  onStats: (stats: ModelStats) => void;
  onError?: (message: string) => void;
}

export function ModelPreview({ path, onStats, onError }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState(false);
  const onStatsRef = useRef(onStats);
  const onErrorRef = useRef(onError);
  onStatsRef.current = onStats;
  onErrorRef.current = onError;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    let disposed = false;
    setError(false);
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x17181b);
    const camera = new THREE.PerspectiveCamera(38, 1, 0.01, 10_000);
    camera.position.set(2.5, 1.8, 3.2);

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.05;
    host.appendChild(renderer.domElement);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;

    scene.add(new THREE.HemisphereLight(0xffffff, 0x30343b, 2.6));
    const key = new THREE.DirectionalLight(0xffffff, 3.5);
    key.position.set(4, 6, 3);
    scene.add(key);
    const rim = new THREE.DirectionalLight(0xc99a45, 1.35);
    rim.position.set(-4, 2, -4);
    scene.add(rim);

    let model: THREE.Object3D | null = null;
    let mixer: THREE.AnimationMixer | null = null;
    let frame = 0;
    let previousTime = performance.now();
    let interacting = false;
    let settleFrames = 0;
    let visible = true;

    const scheduleRender = () => {
      if (!disposed && visible && !document.hidden && frame === 0) {
        frame = requestAnimationFrame(render);
      }
    };
    const render = (time: number) => {
      frame = 0;
      if (disposed) return;
      mixer?.update(Math.min((time - previousTime) / 1000, 0.1));
      previousTime = time;
      controls.update();
      renderer.render(scene, camera);
      if (settleFrames > 0) settleFrames -= 1;
      if (mixer || interacting || settleFrames > 0) scheduleRender();
    };
    const startInteraction = () => {
      interacting = true;
      scheduleRender();
    };
    const finishInteraction = () => {
      interacting = false;
      settleFrames = 24;
      scheduleRender();
    };
    const handleControlChange = () => scheduleRender();
    const handleVisibilityChange = () => {
      if (document.hidden) {
        cancelAnimationFrame(frame);
        frame = 0;
      } else {
        previousTime = performance.now();
        scheduleRender();
      }
    };
    controls.addEventListener("start", startInteraction);
    controls.addEventListener("end", finishInteraction);
    controls.addEventListener("change", handleControlChange);
    document.addEventListener("visibilitychange", handleVisibilityChange);

    const visibilityObserver = new IntersectionObserver(([entry]) => {
      visible = entry?.isIntersecting ?? true;
      if (visible) {
        previousTime = performance.now();
        scheduleRender();
      } else {
        cancelAnimationFrame(frame);
        frame = 0;
      }
    });
    visibilityObserver.observe(host);

    const loader = new GLTFLoader();
    loader.load(
      convertFileSrc(path),
      (gltf) => {
        if (disposed) {
          disposeModel(gltf.scene);
          return;
        }
        model = gltf.scene;
        prepareModelForPreview(model);
        scene.add(model);
        let triangles = 0;
        let vertices = 0;
        model.traverse((child) => {
          if (!(child instanceof THREE.Mesh)) return;
          const position = child.geometry.getAttribute("position");
          const instances = child instanceof THREE.InstancedMesh ? child.count : 1;
          vertices += (position?.count ?? 0) * instances;
          triangles += Math.floor(
            ((child.geometry.index?.count ?? position?.count ?? 0) / 3) * instances,
          );
        });
        onStatsRef.current({ triangles, vertices });
        const box = new THREE.Box3().setFromObject(model);
        const size = box.getSize(new THREE.Vector3());
        const center = box.getCenter(new THREE.Vector3());
        const radius = Math.max(size.x, size.y, size.z) || 1;
        model.position.sub(center);
        camera.near = Math.max(radius / 100, 0.001);
        camera.far = Math.max(radius * 20, camera.near + 1);
        camera.position.set(radius * 1.65, radius * 1.15, radius * 2.1);
        controls.target.set(0, 0, 0);
        camera.updateProjectionMatrix();
        controls.update();
        if (gltf.animations.length > 0) {
          mixer = new THREE.AnimationMixer(model);
          mixer.clipAction(gltf.animations[0]).play();
        }
        previousTime = performance.now();
        scheduleRender();
      },
      undefined,
      (caught) => {
        if (disposed) return;
        const message = caught instanceof Error ? caught.message : "The model loader could not read this file";
        setError(true);
        onErrorRef.current?.(message);
      },
    );

    const resize = () => {
      const width = host.clientWidth;
      const height = host.clientHeight;
      renderer.setSize(width, height, false);
      camera.aspect = Math.max(width / Math.max(height, 1), 0.01);
      camera.updateProjectionMatrix();
      scheduleRender();
    };
    const observer = new ResizeObserver(resize);
    observer.observe(host);
    resize();

    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      observer.disconnect();
      visibilityObserver.disconnect();
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      controls.removeEventListener("start", startInteraction);
      controls.removeEventListener("end", finishInteraction);
      controls.removeEventListener("change", handleControlChange);
      controls.dispose();
      mixer?.stopAllAction();
      if (model) disposeModel(model);
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [path]);

  return (
    <div
      className="model-preview relative mx-3 h-[218px] overflow-hidden rounded-md border bg-muted/10"
      ref={hostRef}
    >
      {error && (
        <div className="absolute inset-0 z-10 grid place-items-center bg-background px-6 text-center text-xs text-muted-foreground">
          Preview unavailable. See diagnostics for details.
        </div>
      )}
    </div>
  );
}
