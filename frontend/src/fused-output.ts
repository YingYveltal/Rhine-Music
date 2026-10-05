import * as THREE from "three";
import type { BokehPass } from "three/addons/postprocessing/BokehPass.js";

/** Fuse the last two fullscreen passes without changing the blur, exposure or
 * display transfer. Keep the original half-float intermediate rounding. */
export function installFusedOutput(pass: BokehPass) {
  const material = pass.materialBokeh;
  material.toneMapped = false;
  material.uniforms.rhineFusedOutput = { value: false };
  material.uniforms.toneMappingExposure = { value: 1 };
  material.fragmentShader = `
    uniform bool rhineFusedOutput;
    ${THREE.ShaderChunk.tonemapping_pars_fragment}
    ${material.fragmentShader.replace("void main()", "void rhineBokehMain()")}
    void main() {
      rhineBokehMain();
      if (rhineFusedOutput) {
        // EffectComposer stores bokeh into RGBA16F before OutputPass. Retain
        // that quantization even though the intermediate texture is removed.
        gl_FragColor.rg = unpackHalf2x16(packHalf2x16(gl_FragColor.rg));
        gl_FragColor.ba = unpackHalf2x16(packHalf2x16(gl_FragColor.ba));
        gl_FragColor.rgb = ACESFilmicToneMapping(gl_FragColor.rgb);
        gl_FragColor = sRGBTransferOETF(gl_FragColor);
      }
    }
  `;
  material.needsUpdate = true;
}
