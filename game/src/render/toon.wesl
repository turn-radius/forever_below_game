// Toon fragment shader for `ToonMaterial` (`ExtendedMaterial<StandardMaterial, ToonExtension>`).
//
// The StandardMaterial inputs (base colour, normal, occlusion) come from Bevy's
// `pbr_input_from_standard_material`. Lighting is our own: each light's N.L is quantised into
// hard bands, as three.js `MeshToonMaterial` does with a 1-pixel gradient map
// (`makeGradient` in `reference/spline_armory_engine.js`), then scaled by the light's colour
// and distance falloff. Wear is painted on the base colour first (`decay.wesl`); both
// work in display (sRGB) values, as the prototype did. The vertex shader is Bevy's mesh
// vertex shader plus one thing: fine parts that get no ink line are fattened to a minimum
// width on screen, so a bowstring stays visible across the room.
// See `docs/rendering.md`.

import bevy_pbr::render::{
    forward_io::{Vertex, VertexOutput, FragmentOutput, decompress_vertex},
    mesh_functions,
    view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    pbr_types::PbrInput,
    pbr_lighting::getDistanceAttenuation,
    mesh_view_bindings as view_bindings,
    mesh_view_types,
    mesh_functions::get_local_from_world,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    clustered_forward as clustering,
    shadows,
};
import bevy_render::maths::PI;
import super::decay::apply_decay;
import super::screen::reference_pixel;

struct ToonUniform {
    // Up to eight band values, packed four to a vec4.
    band_values: array<vec4<f32>, 2>,
    band_count: u32,
    decay_type: u32,
    decay_amount: f32,
    decay_scale: f32,
    decay_seed: f32,
    // Self-lit strength as a fraction of the worn base colour (glowing gems).
    glow: f32,
    // Narrowest the part may look, in pixels on a 1080-pixel-tall viewport; 0 for no minimum.
    min_width_px: f32,
    // The part's own half thickness in item units, so only parts thinner than the minimum
    // are fattened.
    half_thickness: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> toon: ToonUniform;

// Bevy's mesh vertex shader (`bevy_pbr::render::mesh`) without morph targets or skinning,
// which Armory parts never have, plus the minimum width.
@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    let vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);
    var out: VertexOutput;
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_in.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    if toon.min_width_px > 0.0 {
        // Push out along the normal until the part is at least `min_width_px` across. A
        // thicker part, or the same part seen up close, does not move.
        let half_width = 0.5 * toon.min_width_px * reference_pixel(world_position.xyz);
        let push = max(half_width - toon.half_thickness, 0.0);
        world_position = vec4<f32>(world_position.xyz + normalize(out.world_normal) * push, 1.0);
    }
    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);

    @if(VERTEX_UVS_A)
    out.uv = vertex.uv;
    @if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;
    @if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex_in.instance_index);
    @if(VERTEX_COLORS)
    out.color = vertex.color;
    @if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_in.instance_index;
    @if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(vertex_in.instance_index, world_from_local[3]);
    return out;
}

// The band for one light: three.js samples its gradient at N.L * 0.5 + 0.5, so the unlit
// side still gets the lowest band. A shadowed fragment drops to the lowest band.
fn toon_band(n_dot_l: f32, shadow: f32) -> f32 {
    let coord = clamp(n_dot_l * 0.5 + 0.5, 0.0, 1.0) * shadow;
    let count = max(toon.band_count, 1u);
    let index = min(u32(floor(coord * f32(count))), count - 1u);
    return toon.band_values[index / 4u][index % 4u];
}

// Radiance from every point, spot and directional light reaching this fragment, banded per
// light. Same light data and units as Bevy's `apply_pbr_lighting`, with a Lambert lobe.
fn toon_direct_light(in: PbrInput) -> vec3<f32> {
    let N = in.N;
    let P = in.world_position.xyz;
    let receives_shadows = (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u;

    let view_z = dot(vec4<f32>(
        view_bindings::view.view_from_world[0].z,
        view_bindings::view.view_from_world[1].z,
        view_bindings::view.view_from_world[2].z,
        view_bindings::view.view_from_world[3].z
    ), in.world_position);
    let cluster_index = clustering::view_fragment_cluster_index(in.frag_coord.xy, view_z, in.is_orthographic);
    let ranges = clustering::unpack_clusterable_object_index_ranges(cluster_index);

    var light = vec3<f32>(0.0);

    // Point lights, then spot lights (they share the clustered light list).
    for (var i: u32 = ranges.first_point_light_index_offset;
            i < ranges.first_rect_light_index_offset;
            i = i + 1u) {
        let light_id = clustering::get_clusterable_object_id(i);
        let clustered = &view_bindings::clustered_lights.data[light_id];
        let is_spot = i >= ranges.first_spot_light_index_offset;

        let light_to_frag = (*clustered).position_radius.xyz - P;
        let L = normalize(light_to_frag);
        let attenuation = getDistanceAttenuation(
            dot(light_to_frag, light_to_frag),
            (*clustered).color_inverse_square_range.w,
        );

        var shadow = 1.0;
        if receives_shadows
                && ((*clustered).flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            if is_spot {
                shadow = shadows::fetch_spot_shadow(
                    light_id, in.world_position, in.world_normal, (*clustered).shadow_map_near_z, in.frag_coord.xy);
            } else {
                shadow = shadows::fetch_point_shadow(light_id, in.world_position, in.world_normal, in.frag_coord.xy);
            }
        }

        // Spot cone, as in Bevy's `spot_light`.
        var cone = 1.0;
        if is_spot {
            var spot_dir = vec3<f32>((*clustered).light_custom_data.x, 0.0, (*clustered).light_custom_data.y);
            spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
            if ((*clustered).flags & mesh_view_types::POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u {
                spot_dir.y = -spot_dir.y;
            }
            let cd = dot(-spot_dir, L);
            let falloff = saturate(cd * (*clustered).light_custom_data.z + (*clustered).light_custom_data.w);
            cone = falloff * falloff;
        }

        light += (*clustered).color_inverse_square_range.rgb * attenuation * cone * toon_band(dot(N, L), shadow);
    }

    for (var i: u32 = 0u; i < view_bindings::lights.n_directional_lights; i = i + 1u) {
        let directional = &view_bindings::lights.directional_lights[i];
        var shadow = 1.0;
        if receives_shadows
                && ((*directional).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            shadow = shadows::fetch_directional_shadow(i, in.world_position, in.world_normal, view_z, in.frag_coord.xy);
            shadow *= in.directional_shadow_factor;
        }
        light += (*directional).color.rgb * toon_band(dot(N, (*directional).direction_to_light), shadow);
    }

    // Lambert: the band stands in for N.L, 1/PI as in Bevy's diffuse lobe.
    return light * (1.0 / PI);
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3(0.0));
    return select(1.055 * pow(x, vec3(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3(0.0031308));
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3(0.0));
    return select(pow((x + 0.055) / 1.055, vec3(2.4)), x / 12.92, x <= vec3(0.04045));
}

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(vertex_output, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );

    // Item space: parts sit at the item root with no transform of their own, so the mesh's
    // local space is the item's.
    let item_position = (get_local_from_world(vertex_output.instance_index) * vertex_output.world_position).xyz;
    // Wear and light both work on the display (sRGB) colour, as three.js r128 did with no
    // output encoding: the prototype's wear colours are display values, and a band at half
    // light reads as half as bright on screen, which keeps the steps crisp.
    let base = apply_decay(
        linear_to_srgb(pbr_input.material.base_color.rgb),
        item_position,
        toon.decay_type,
        toon.decay_amount,
        toon.decay_scale,
        toon.decay_seed,
    );

    let ambient = view_bindings::lights.ambient_color.rgb * pbr_input.diffuse_occlusion;
    let light = (toon_direct_light(pbr_input) + ambient) * view_bindings::view.exposure;
    let lit = srgb_to_linear(base * light);
    // Glow fades as the stone clouds over (`gemMat`).
    let glow = srgb_to_linear(base) * toon.glow * (1.0 - 0.8 * clamp(toon.decay_amount, 0.0, 1.0));

    var out: FragmentOutput;
    out.color = vec4<f32>(lit + glow, pbr_input.material.base_color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
