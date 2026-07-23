// src/render/BuildingLayer.ts
// One deck.gl layer for ALL rendered buildings: a single merged indexed mesh
// with per-vertex colour and a per-vertex feature index for picking. Replaces
// one SimpleMeshLayer per feature, which hit deck's 255-pickable-layer cap and
// cost one draw call per building.
import {
  type DefaultProps, type GetPickingInfoParams, type LayerProps,
  type UpdateParameters, Layer, gouraudMaterial, picking, project32,
} from '@deck.gl/core'
import { Geometry, Model } from '@luma.gl/engine'
import type { RenderedFeature } from '../store/index'
import type { MergedMesh } from './mergeFeatures'

const vs = `#version 300 es
#define SHADER_NAME building-layer-vertex
in vec3 positions;
in vec3 normals;
in vec4 colors;
in float pickIndex;
out vec4 vColor;
void main(void) {
  geometry.worldPosition = positions;
  geometry.normal = project_normal(normals);
  // Encode the feature index into the picking colour (deck decodes index+1).
  float idx = pickIndex + 1.0;
  geometry.pickingColor = vec3(
    mod(idx, 256.0),
    mod(floor(idx / 256.0), 256.0),
    floor(idx / 65536.0)
  );
  vec4 position_commonspace;
  gl_Position = project_position_to_clipspace(positions, vec3(0.0), vec3(0.0), position_commonspace);
  geometry.position = position_commonspace;
  DECKGL_FILTER_GL_POSITION(gl_Position, geometry);
  vec3 lightColor = lighting_getLightColor(colors.rgb, project.cameraPosition, geometry.position.xyz, geometry.normal);
  vColor = vec4(lightColor, colors.a * layer.opacity);
  DECKGL_FILTER_COLOR(vColor, geometry);
}
`

const fs = `#version 300 es
#define SHADER_NAME building-layer-fragment
precision highp float;
in vec4 vColor;
out vec4 fragColor;
void main(void) {
  fragColor = vColor;
  DECKGL_FILTER_COLOR(fragColor, geometry);
}
`

export type BuildingLayerProps = {
  /** The merged mesh (positions in lng/lat, per-vertex colours + feature ids). */
  mesh: MergedMesh
  /** Indexed by the mesh's per-vertex feature id, for picking. */
  features: RenderedFeature[]
} & LayerProps

const EMPTY_MESH: MergedMesh = {
  positions: { size: 3, value: new Float32Array() },
  normals: { size: 3, value: new Float32Array() },
  colors: { size: 4, value: new Float32Array() },
  pickIndex: { size: 1, value: new Float32Array() },
  indices: { size: 1, value: new Uint32Array() },
  vertexCount: 0,
}

const defaultProps: DefaultProps<BuildingLayerProps> = {
  mesh: { type: 'object', value: EMPTY_MESH, compare: true },
  features: { type: 'array', value: [], compare: false },
}

interface State { model?: Model }

export class BuildingLayer extends Layer<Required<BuildingLayerProps>> {
  static layerName = 'BuildingLayer'
  static defaultProps = defaultProps

  getShaders() {
    return super.getShaders({ vs, fs, modules: [project32, gouraudMaterial, picking] })
  }

  initializeState(): void {
    // Attributes live in the Geometry (a single pre-built mesh), not the
    // AttributeManager, so there is nothing to register here.
  }

  updateState(params: UpdateParameters<this>): void {
    super.updateState(params)
    const { props, oldProps, changeFlags } = params
    if (changeFlags.extensionsChanged || props.mesh !== oldProps.mesh) {
      ;(this.state as State).model?.destroy()
      this.setState({
        model: props.mesh.vertexCount > 0 ? this.buildModel(props.mesh) : undefined,
      })
    }
  }

  getModels(): Model[] {
    const m = (this.state as State).model
    return m ? [m] : []
  }

  draw(): void {
    ;(this.state as State).model?.draw(this.context.renderPass)
  }

  getPickingInfo(params: GetPickingInfoParams): GetPickingInfoParams['info'] {
    const info = super.getPickingInfo(params)
    if (info.index >= 0 && info.index < this.props.features.length) {
      info.object = this.props.features[info.index]
    }
    return info
  }

  private buildModel(mesh: MergedMesh): Model {
    return new Model(this.context.device, {
      ...this.getShaders(),
      id: this.props.id,
      geometry: new Geometry({
        topology: 'triangle-list',
        indices: mesh.indices,
        attributes: {
          positions: mesh.positions,
          normals: mesh.normals,
          colors: mesh.colors,
          pickIndex: mesh.pickIndex,
        },
      }),
      isInstanced: false,
    })
  }
}
