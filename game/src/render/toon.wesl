// Toon fragment shader for `ToonMaterial` (`ExtendedMaterial<StandardMaterial, ToonExtension>`).
//
// The StandardMaterial inputs (base colour, normal, occlusion) come from Bevy's
// `pbr_input_from_standard_material`. Lighting is our own: each light's N.L is quantised into
// hard bands, as three.js `MeshToonMaterial` does with a 1-pixel gradient map
// (`makeGradient` in `reference/spline_armory_engine.js`), then scaled by the light's colour.
// Point and spot lights also step down in hard steps across their reach (`reach_falloff`), and
// flames flicker on top of their steps (`flame_flicker`). Wear is painted on the base colour first (`decay.wesl`); both
// work in display (sRGB) values, as the prototype did. The vertex shader is Bevy's mesh
// vertex shader plus one thing: fine parts that get no ink line are fattened to a minimum
// width on screen, so a bowstring stays visible across the room.
// See `docs/rendering.md`.

import bevy_pbr::render::{
    forward_io::{Vertex, VertexOutput, FragmentOutput, decompress_vertex},
    mesh_functions,
    mesh_bindings::mesh,
    view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    pbr_types::PbrInput,
    pbr_lighting::getRangeFalloff,
    mesh_view_bindings as view_bindings,
    mesh_view_types,
    mesh_functions::get_local_from_world,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    clustered_forward as clustering,
    shadows,
};
import bevy_render::maths::PI;
import super::decay::apply_decay;
import super::pattern::{Wrap, apply_pattern};
import super::etch::{Etch, apply_etch};
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
    // The part's surface pattern (`pattern.wesl`): its second colour (display values), its id
    // (0 for none), and where a wrapped pattern sits on the part.
    pattern_color: vec3<f32>,
    pattern_id: u32,
    pattern_origin: vec3<f32>,
    pattern_scale: f32,
    pattern_axis: vec3<f32>,
    pattern_base: f32,
    pattern_zero: vec3<f32>,
    pattern_around: f32,
    pattern_mirrored: u32,
    pattern_length: f32,
    pattern_profile: array<vec4<f32>, 4>,
    // The part's etching (`etch.wesl`): inlay or thread colour, id (0 for none), scale, line
    // weight, coverage, whether inlaid, how dark a blackened line is, and whether stitched.
    etch_color: vec3<f32>,
    etch_id: u32,
    etch_scale: f32,
    etch_weight: f32,
    etch_coverage: f32,
    etch_inlaid: u32,
    etch_darken: f32,
    etch_stitched: u32,
}

// Bindless where the device allows it (`ToonExtension`'s `bindless` attribute), as Bevy's
// `pbr_bindings` does for `StandardMaterial`: every part's uniform sits in one array, found
// through the material slot in the mesh's uniform, so parts share one bind group and draw in
// batches, shadow passes included.
@if(BINDLESS) {
struct ToonBindings {
    material: u32,
}
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var<storage> toon_indices: array<ToonBindings>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<storage> toon_array: array<ToonUniform>;
}
@else {
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> toon_uniform: ToonUniform;
}

// This part's toon uniform.
fn toon_of(instance_index: u32) -> ToonUniform {
@if(BINDLESS)
    let toon = toon_array[toon_indices[mesh[instance_index].material_and_lightmap_bind_group_slot & 0xffffu].material];
@else
    let toon = toon_uniform;
    return toon;
}

// Bevy's mesh vertex shader (`bevy_pbr::render::mesh`) without morph targets or skinning,
// which Armory parts never have, plus the minimum width.
@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    let vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);
    let toon = toon_of(vertex_in.instance_index);
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
fn toon_band(toon: ToonUniform, n_dot_l: f32, shadow: f32) -> f32 {
    let coord = clamp(n_dot_l * 0.5 + 0.5, 0.0, 1.0) * shadow;
    let count = max(toon.band_count, 1u);
    let index = min(u32(floor(coord * f32(count))), count - 1u);
    return toon.band_values[index / 4u][index % 4u];
}

// Fraction of a light's range inside which its falloff stops climbing. Must match
// `light::CORE_FRACTION`.
const CORE_FRACTION: f32 = 0.4;
// Steps a point or spot light's strength falls in across its reach, from full to dark.
const REACH_STEPS: f32 = 4.0;

// Distance attenuation of a point or spot light, in place of Bevy's `getDistanceAttenuation`
// (`1 / d^2` windowed). The window `(1 - (d/r)^4)^2` and the `+1` that keeps the inverse square
// finite at the light are from Brian Karis, "Real Shading in Unreal Engine 4" (SIGGRAPH 2013,
// eq. 9): `saturate(1 - (d/r)^4)^2 / (d^2 + 1)`. Here the `+1` is `core^2` with
// `core = CORE_FRACTION * r` (our own choice), so a light no longer climbs without bound at
// its source and every light has the same shape across its reach. Mirrors
// `light::reach_falloff`, which is this times `core^2`.
fn reach_attenuation(distance_squared: f32, inverse_range_squared: f32) -> f32 {
    let core_squared = CORE_FRACTION * CORE_FRACTION / max(inverse_range_squared, 1e-12);
    return getRangeFalloff(distance_squared, inverse_range_squared) / (distance_squared + core_squared);
}

// One point or spot light's display-space strength (`radiance` per channel, exposed) cut to
// `REACH_STEPS` hard steps: capped at full, so a wall beside the flame is full light and not
// burnt white, then rounded up, so the dimmest step runs out to the light's range and ends
// there, as dim light ends at the edge of a torch's reach. Keeps the light's hue.
fn reach_step(radiance: vec3<f32>) -> vec3<f32> {
    let level = max(radiance.r, max(radiance.g, radiance.b));
    if level <= 0.0 {
        return vec3(0.0);
    }
    let stepped = ceil(min(level, 1.0) * REACH_STEPS) / REACH_STEPS;
    return radiance * (stepped / level);
}

// How far a flame's flicker lags per world unit along x and z, in seconds, so neighbouring
// flames waver out of step: braziers two cells (40 units) apart are about 0.3 s apart, half a
// period of the curve's main term. Linear in position, so a carried light's flicker stays
// smooth as it moves (walking shifts its rate by about a tenth). Height is left out so the
// bob of a carried torch does not touch it. Our own tuning.
const FLICKER_LAG: vec3<f32> = vec3(0.0071, 0.0, 0.0053);

// A flame's strength multiplier now: the prototype's flicker
// `.88 + .08 sin(13.1t) + .05 sin(7.3t + 1.2) + .03 sin(23.7t)` (`docs/rendering.md`, "Light"),
// scaled toward 1 by the light's flicker depth, 0 for a steady light. The depth rides in the
// light's radius, which Bevy otherwise uses only for specular and soft shadows, neither drawn
// here (`light::Torch`). Applied after `reach_step`, so the steps hold still and only their
// brightness wavers. Mirrors `light::Torch::wavering`.
fn flame_flicker(position_radius: vec4<f32>) -> f32 {
    let depth = clamp(position_radius.w, 0.0, 1.0);
    if depth <= 0.0 {
        return 1.0;
    }
    let t = view_bindings::globals.time + dot(position_radius.xyz, FLICKER_LAG);
    let curve = 0.88 + 0.08 * sin(13.1 * t) + 0.05 * sin(7.3 * t + 1.2) + 0.03 * sin(23.7 * t);
    return 1.0 - depth * (1.0 - curve);
}

// Radiance from every point, spot and directional light reaching this fragment, banded per
// light, times the view's exposure. Same light data and units as Bevy's
// `apply_pbr_lighting`, with a Lambert lobe.
fn toon_direct_light(toon: ToonUniform, in: PbrInput) -> vec3<f32> {
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

    let exposure = view_bindings::view.exposure;
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
        let attenuation = reach_attenuation(
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

        // Lambert: the band stands in for N.L, 1/PI as in Bevy's diffuse lobe.
        let radiance = (*clustered).color_inverse_square_range.rgb * attenuation * cone * (exposure / PI);
        light += reach_step(radiance) * flame_flicker((*clustered).position_radius) * toon_band(toon, dot(N, L), shadow);
    }

    for (var i: u32 = 0u; i < view_bindings::lights.n_directional_lights; i = i + 1u) {
        let directional = &view_bindings::lights.directional_lights[i];
        var shadow = 1.0;
        if receives_shadows
                && ((*directional).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            shadow = shadows::fetch_directional_shadow(i, in.world_position, in.world_normal, view_z, in.frag_coord.xy);
            shadow *= in.directional_shadow_factor;
        }
        light += (*directional).color.rgb * (exposure / PI) * toon_band(toon, dot(N, (*directional).direction_to_light), shadow);
    }

    return light;
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
    let toon = toon_of(vertex_output.instance_index);
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
    // Patterns go on first and wear over them, as in the prototype's `DECAY_BODY`.
    var uv = vec2<f32>(0.0);
    @if(VERTEX_UVS_A)
    uv = vertex_output.uv;
    let patterned = apply_pattern(
        linear_to_srgb(pbr_input.material.base_color.rgb),
        item_position,
        uv,
        length(fwidth(item_position)) * toon.pattern_scale,
        toon.pattern_id,
        toon.pattern_color,
        Wrap(
            toon.pattern_origin,
            toon.pattern_axis,
            toon.pattern_zero,
            toon.pattern_base,
            toon.pattern_scale,
            toon.pattern_around,
            toon.pattern_mirrored,
            toon.pattern_length,
            toon.pattern_profile,
        ),
    );
    // Etching after the pattern, wear over both (`DECAY_BODY`).
    let item_normal = normalize((get_local_from_world(vertex_output.instance_index) * vec4<f32>(pbr_input.world_normal, 0.0)).xyz);
    let pixel = length(fwidth(item_position));
    let etched = apply_etch(
        patterned,
        item_position,
        item_normal,
        pixel,
        Etch(
            toon.etch_id,
            toon.etch_color,
            toon.etch_scale,
            toon.etch_weight,
            toon.etch_coverage,
            toon.etch_inlaid,
            toon.etch_darken,
            toon.etch_stitched,
        ),
        toon.decay_amount,
        toon.decay_scale,
        toon.decay_seed,
    );
    let base = apply_decay(
        etched,
        item_position,
        pixel,
        toon.decay_type,
        toon.decay_amount,
        toon.decay_scale,
        toon.decay_seed,
    );

    let ambient = view_bindings::lights.ambient_color.rgb * pbr_input.diffuse_occlusion;
    let light = toon_direct_light(toon, pbr_input) + ambient * view_bindings::view.exposure;
    let lit = srgb_to_linear(base * light);
    // Glow fades as the stone clouds over (`gemMat`).
    let glow = srgb_to_linear(base) * toon.glow * (1.0 - 0.8 * clamp(toon.decay_amount, 0.0, 1.0));

    var out: FragmentOutput;
    // See-through glass (alpha below 1) stops more light where it is seen edge on, as a
    // curved pane looks thicker at its rim. A cubic of 1 - |N.V| is our own stand-in for
    // Fresnel, here only so a bottle's outline reads inside its ink line.
    var alpha = pbr_input.material.base_color.a;
    if alpha < 1.0 {
        let to_eye = normalize(view_bindings::view.world_position - pbr_input.world_position.xyz);
        let edge = 1.0 - abs(dot(pbr_input.N, to_eye));
        alpha = mix(alpha, 0.85, edge * edge * edge);
    }
    out.color = vec4<f32>(lit + glow, alpha);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
