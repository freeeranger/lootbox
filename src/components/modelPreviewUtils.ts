import * as THREE from "three";

/**
 * GLTF blend materials are sorted per object, not per triangle. That breaks down
 * for common game-asset geometry such as crossed foliage cards or layered decals.
 * Alpha hashing keeps partial transparency while letting the depth buffer resolve
 * those surfaces in either viewing direction.
 */
export function prepareModelForPreview(model: THREE.Object3D) {
  model.traverse((child) => {
    if (!(child instanceof THREE.Mesh)) return;
    const materials = Array.isArray(child.material) ? child.material : [child.material];
    for (const material of materials) {
      if (!material.transparent) continue;
      material.transparent = false;
      material.alphaHash = true;
      material.depthTest = true;
      material.depthWrite = true;
      material.needsUpdate = true;
    }
  });
}

export function disposeModel(model: THREE.Object3D) {
  model.traverse((child) => {
    if (!(child instanceof THREE.Mesh)) return;
    child.geometry?.dispose();
    const materials = Array.isArray(child.material) ? child.material : [child.material];
    materials.forEach((material) => {
      Object.values(material).forEach((value) => {
        if (value instanceof THREE.Texture) value.dispose();
      });
      material.dispose();
    });
  });
}
