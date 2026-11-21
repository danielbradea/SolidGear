wit_bindgen::generate!({ path: "wit", world: "command-plugin" });

use craft::plugin::types::{FieldFailure, PluginError};
use craft::plugin::{commands, log};
use exports::craft::plugin::entry::Guest;
use serde_json::{Value, json};
use std::f64::consts::{PI, TAU};

struct SolidGear;

impl Guest for SolidGear {
    fn execute(entry: String, params: String) -> Result<String, PluginError> {
        let mut params: Value =
            serde_json::from_str(&params).map_err(|error| invalid("params", error.to_string()))?;
        let Some(fields) = params.as_object_mut() else {
            return Err(invalid("params", "Expected an object."));
        };
        // The numeric prefix controls display order independently of the input name.
        let ordered_fields: Vec<_> = fields
            .iter()
            .filter_map(|(key, value)| {
                let (prefix, name) = key.split_once('_')?;
                (!prefix.is_empty() && prefix.bytes().all(|byte| byte.is_ascii_digit()))
                    .then(|| (name.to_owned(), value.clone()))
            })
            .collect();
        for (name, value) in ordered_fields {
            fields.insert(name, value);
        }
        match entry.as_str() {
            "involute-gear" => generate_involute_gear(&params),
            "internal-involute-gear" => generate_internal_gear(&params),
            "involute-rack" => generate_rack(&params),
            "planetary-gear" => generate_planetary_gear(&params),
            _ => Err(invalid("entry", "Unknown SolidGear tool.")),
        }
    }
}

fn generate_planetary_gear(params: &Value) -> Result<String, PluginError> {
    let sun = tooth_count(params, "sun_teeth", 24.0)? as u32;
    let planet = tooth_count(params, "planet_teeth", 18.0)? as u32;
    let ring = sun + 2 * planet;
    if ring > 120 {
        return Err(invalid(
            "planet_teeth",
            "Sun teeth plus twice the planet teeth must be at most 120.",
        ));
    }
    let count = number(params, "planet_count", 3.0)?;
    if count.fract() != 0.0 || !(1.0..=6.0).contains(&count) {
        return Err(invalid("planet_count", "Enter a whole number from 1 to 6."));
    }
    let count = count as u32;
    if (sun + ring) % count != 0 {
        return Err(invalid(
            "planet_count",
            "For equally spaced planets, the sum of the sun and ring tooth counts must be divisible by the planet count.",
        ));
    }
    let (module, pressure, width) = spur_dimensions(params)?;
    let rim = positive(params, "rim_thickness", 5.0)?;
    let backlash = number(params, "backlash", 0.1)?;
    if backlash < 0.0 {
        return Err(invalid("backlash", "Backlash cannot be negative."));
    }
    let sun_bore = number(params, "sun_bore", 8.0)?;
    let planet_bore = number(params, "planet_bore", 5.0)?;
    let orbit = module * f64::from(sun + planet) / 2.0;
    if count > 1
        && 2.0 * orbit * (PI / f64::from(count)).sin() <= module * (f64::from(planet) + 2.0)
    {
        return Err(invalid(
            "planet_count",
            "Adjacent planets need more clearance.",
        ));
    }
    let origin = [
        number(params, "position_x", 0.0)?,
        number(params, "position_y", 0.0)?,
        number(params, "position_z", 0.0)?,
    ];
    let rotation = number(params, "rotation", 0.0)?
        .rem_euclid(360.0)
        .to_radians();
    let allowance = backlash / 2.0;
    let helix = helix_angle(params)?.to_degrees();
    let double_helix = params["double_helix"].as_bool() == Some(true) && helix != 0.0;
    let ring_phase = if planet % 2 == 0 {
        PI / f64::from(ring)
    } else {
        0.0
    };
    let crown = generate_internal_gear(&json!({
        "teeth": ring, "module": module, "pressure_angle": pressure.to_degrees(),
        "face_width": width, "rim_thickness": rim, "backlash": allowance,
        "position_x": origin[0], "position_y": origin[1], "position_z": origin[2],
        "rotation": (rotation + ring_phase).to_degrees(), "helix_angle": helix,
        "double_helix": double_helix
    }))?;
    let sun_gear = create_gear(
        f64::from(sun),
        module,
        pressure.to_degrees(),
        width,
        (sun_bore, "sun_bore", BoreShape::Round),
        (allowance, sun <= 80, 0.0, 0.0, 0.0),
        origin,
        (rotation, helix, double_helix),
    )?;
    let mut planets = Vec::new();
    for index in 0..count {
        let azimuth = TAU * f64::from(index) / f64::from(count);
        let offset = polar(orbit, rotation + azimuth);
        let center = [origin[0] + offset[0], origin[1] + offset[1], origin[2]];
        // At each sun contact, put a tooth opposite the corresponding gap.
        let phase = rotation + azimuth + PI + (f64::from(sun) * azimuth - PI) / f64::from(planet);
        planets.push(create_gear(
            f64::from(planet),
            module,
            pressure.to_degrees(),
            width,
            (planet_bore, "planet_bore", BoreShape::Round),
            (allowance, planet <= 80, 0.0, 0.0, 0.0),
            center,
            (phase, -helix, double_helix),
        )?);
    }
    Ok(json!({
        "ring_teeth": ring, "sun_planet_distance": orbit, "helix_angle": helix,
        "double_helix": double_helix,
        "reduction_ring_fixed": 1.0 + f64::from(ring) / f64::from(sun),
        "ring": serde_json::from_str::<Value>(&crown).map_err(|error| PluginError::Trap(error.to_string()))?,
        "sun": sun_gear, "planets": planets
    }).to_string())
}

fn generate_internal_gear(params: &Value) -> Result<String, PluginError> {
    let teeth = tooth_count(params, "teeth", 48.0)?;
    let (module, pressure, width) = spur_dimensions(params)?;
    let rim = positive(params, "rim_thickness", 5.0)?;
    let fillet = root_fillet(params)?;
    let backlash = number(params, "backlash", 0.0)?;
    if backlash < 0.0 {
        return Err(invalid("backlash", "Backlash cannot be negative."));
    }
    let pitch = module * teeth / 2.0;
    let tip = pitch - module;
    let root = pitch + 1.25 * module;
    let outer = root + rim;
    if !(2.0 * outer).is_finite() {
        return Err(invalid("module", "The resulting diameter is too large."));
    }
    let base = pitch * pressure.cos();
    let period = TAU / teeth;
    // Build the boundary of the opening. Its gaps are the spaces between
    // inward-facing teeth, and widen toward the center of the ring.
    let half_gap = |radius: f64| {
        let angle = (base / radius).clamp(0.0, 1.0).acos();
        period / 4.0 + involute(pressure) - involute(angle) + backlash / (2.0 * pitch)
    };
    let tip_gap = half_gap(tip);
    let root_gap = half_gap(root);
    if root_gap <= 0.0 || tip_gap >= period / 2.0 {
        return Err(invalid(
            "teeth",
            "This tooth count, pressure angle and backlash do not leave room for the tooth profile.",
        ));
    }
    let helix = helix_angle(params)?;
    let double_helix = params["double_helix"].as_bool() == Some(true) && helix != 0.0;
    // The mirrored sweep doubles the number of flank faces.
    let segments = if double_helix {
        2
    } else if fillet > 0.0 {
        4
    } else {
        8
    };
    let mut opening = Vec::new();
    let mut corners = Vec::new();
    for tooth in 0..teeth as usize {
        let center = tooth as f64 * period;
        for step in 0..=segments {
            let radius = tip + (root - tip) * f64::from(step) / f64::from(segments);
            opening.push(polar(radius, center - half_gap(radius)));
        }
        corners.push(opening.len() - 1);
        if !double_helix {
            opening.push(polar(root, center));
        }
        corners.push(opening.len());
        for step in (0..=segments).rev() {
            let radius = tip + (root - tip) * f64::from(step) / f64::from(segments);
            opening.push(polar(radius, center + half_gap(radius)));
        }
        if !double_helix {
            opening.push(polar(tip, center + period / 2.0));
        }
    }
    let twist = (width / pitch * helix.tan()).to_degrees();
    if !twist.is_finite() || twist.abs() > 1440.0 {
        return Err(invalid(
            "helix_angle",
            "This width and angle require more than four turns.",
        ));
    }
    let opening = round_root_corners(opening, &corners, fillet)?;
    extrude_outline(
        params,
        opening,
        Some(outer),
        width,
        twist,
        0.0,
        double_helix,
    )?;
    Ok(json!({
        "message": "Internal Involute Gear",
        "pitch_diameter": 2.0 * pitch, "tip_diameter": 2.0 * tip,
        "root_diameter": 2.0 * root, "outer_diameter": 2.0 * outer,
        "helix_angle": helix.to_degrees(), "double_helix": double_helix
    })
    .to_string())
}

fn generate_rack(params: &Value) -> Result<String, PluginError> {
    let teeth = number(params, "teeth", 16.0)?;
    if teeth.fract() != 0.0 || !(1.0..=120.0).contains(&teeth) {
        return Err(invalid("teeth", "Enter a whole number from 1 to 120."));
    }
    let (module, pressure, width) = spur_dimensions(params)?;
    let base_height = positive(params, "base_height", 5.0)?;
    let fillet = root_fillet(params)?;
    let backlash = number(params, "backlash", 0.0)?;
    if backlash < 0.0 {
        return Err(invalid("backlash", "Backlash cannot be negative."));
    }
    let pitch = PI * module;
    let length = teeth * pitch;
    let root = -1.25 * module;
    let bottom = root - base_height;
    if !length.is_finite() || !bottom.is_finite() {
        return Err(invalid("module", "The resulting rack is too large."));
    }
    let pitch_half_width = (pitch / 2.0 - backlash) / 2.0;
    let at_root = pitch_half_width - root * pressure.tan();
    let at_tip = pitch_half_width - module * pressure.tan();
    if at_tip <= 0.0 || at_root >= pitch / 2.0 {
        return Err(invalid(
            "pressure_angle",
            "The pressure angle and backlash leave no clearance between teeth or no tooth tip.",
        ));
    }
    let mut outline = vec![[0.0, bottom], [0.0, root]];
    let mut corners = Vec::new();
    for tooth in 0..teeth as usize {
        let center = (tooth as f64 + 0.5) * pitch;
        corners.extend([outline.len(), outline.len() + 3]);
        outline.extend([
            [center - at_root, root],
            [center - at_tip, module],
            [center + at_tip, module],
            [center + at_root, root],
        ]);
    }
    outline.extend([[length, root], [length, bottom]]);
    let outline = round_root_corners(outline, &corners, fillet)?;
    let helix = helix_angle(params)?;
    let double_helix = params["double_helix"].as_bool() == Some(true) && helix != 0.0;
    let offset = width * helix.tan();
    if !offset.is_finite() {
        return Err(invalid(
            "helix_angle",
            "The resulting tooth offset is too large.",
        ));
    }
    extrude_outline(params, outline, None, width, 0.0, offset, double_helix)?;
    Ok(json!({
        "message": "Involute Rack", "pitch": pitch, "length": length,
        "height": module - bottom, "helix_angle": helix.to_degrees(),
        "tooth_offset": if double_helix { 0.0 } else { offset },
        "double_helix": double_helix
    })
    .to_string())
}

fn spur_dimensions(params: &Value) -> Result<(f64, f64, f64), PluginError> {
    let module = positive(params, "module", 2.0)?;
    let width = positive(params, "face_width", 10.0)?;
    let angle = number(params, "pressure_angle", 20.0)?;
    if !(0.0 < angle && angle < 90.0) {
        return Err(invalid(
            "pressure_angle",
            "Enter an angle between 0° and 90°.",
        ));
    }
    Ok((module, angle.to_radians(), width))
}

fn positive(params: &Value, field: &str, default: f64) -> Result<f64, PluginError> {
    let value = number(params, field, default)?;
    if value <= 0.0 {
        return Err(invalid(field, "Enter a positive value."));
    }
    Ok(value)
}

fn helix_angle(params: &Value) -> Result<f64, PluginError> {
    let angle = number(params, "helix_angle", 0.0)?;
    if angle.abs() >= 90.0 {
        return Err(invalid(
            "helix_angle",
            "Use a helix angle between -90° and 90°.",
        ));
    }
    Ok(angle.to_radians())
}

fn root_fillet(params: &Value) -> Result<f64, PluginError> {
    let radius = number(params, "root_fillet", 0.0)?;
    if radius < 0.0 {
        return Err(invalid("root_fillet", "The radius cannot be negative."));
    }
    Ok(radius)
}

fn round_root_corners(
    outline: Vec<[f64; 2]>,
    corners: &[usize],
    radius: f64,
) -> Result<Vec<[f64; 2]>, PluginError> {
    if radius == 0.0 {
        return Ok(outline);
    }

    let mut rounded = Vec::with_capacity(outline.len() + 2 * corners.len());
    let mut corners = corners.iter().copied().peekable();
    let count = outline.len();
    for (index, &[x, y]) in outline.iter().enumerate() {
        if corners.peek() != Some(&index) {
            rounded.push([x, y]);
            continue;
        }
        corners.next();
        let previous = outline[(index + count - 1) % count];
        let next = outline[(index + 1) % count];
        let (ix, iy) = (x - previous[0], y - previous[1]);
        let (ox, oy) = (next[0] - x, next[1] - y);
        let incoming = ix.hypot(iy);
        let outgoing = ox.hypot(oy);
        let (ix, iy) = (ix / incoming, iy / incoming);
        let (ox, oy) = (ox / outgoing, oy / outgoing);
        let turn = (ix * oy - iy * ox).atan2(ix * ox + iy * oy);
        let tangent = radius * (turn.abs() / 2.0).tan();
        // Adjacent rounds may share an edge. Leave at least 2% of each
        // segment untouched even when both endpoints are rounded.
        if !tangent.is_finite() || tangent > 0.49 * incoming.min(outgoing) {
            return Err(invalid(
                "root_fillet",
                "The radius is too large for the tooth root. Try a smaller value.",
            ));
        }
        let start = [x - ix * tangent, y - iy * tangent];
        let side = turn.signum();
        let center = [start[0] - side * iy * radius, start[1] + side * ix * radius];
        let angle = (start[1] - center[1]).atan2(start[0] - center[0]);
        let middle = polar(radius, angle + turn / 2.0);
        rounded.extend([
            start,
            [center[0] + middle[0], center[1] + middle[1]],
            [x + ox * tangent, y + oy * tangent],
        ]);
    }
    Ok(rounded)
}

fn extrude_outline(
    params: &Value,
    points: Vec<[f64; 2]>,
    outer_circle: Option<f64>,
    width: f64,
    twist: f64,
    offset: f64,
    double_helix: bool,
) -> Result<(), PluginError> {
    let origin = [
        number(params, "position_x", 0.0)?,
        number(params, "position_y", 0.0)?,
        number(params, "position_z", 0.0)?,
    ];
    let (sin, cos) = number(params, "rotation", 0.0)?
        .rem_euclid(360.0)
        .to_radians()
        .sin_cos();
    let sketch = run(
        "sketch.create",
        json!({
            "plane": {"origin": origin, "x_dir": [cos, sin, 0.0], "y_dir": [-sin, cos, 0.0]}
        }),
    )?;
    run(
        "sketch.line",
        json!({"points": points, "closed": true, "infer": false, "connect": false}),
    )?;
    if let Some(radius) = outer_circle {
        run(
            "sketch.circle.center",
            json!({"center": [0, 0], "radius": radius}),
        )?;
    }
    run("sketch.finish", json!({}))?;
    let fraction = if double_helix { 0.5 } else { 1.0 };
    let span = width * fraction;
    let body = if twist != 0.0 {
        sweep_gear(sketch["sketch"].clone(), origin, span, twist * fraction)?
    } else if offset != 0.0 {
        let path = run(
            "sketch.create",
            json!({
                "plane": {"origin": origin, "x_dir": [cos, sin, 0.0], "y_dir": [0, 0, 1]}
            }),
        )?;
        let line = run(
            "sketch.line",
            json!({"points": [[0.0, 0.0], [offset * fraction, span]], "infer": false}),
        )?;
        run("sketch.finish", json!({}))?;
        run(
            "solid.sweep",
            json!({
                "sketch": sketch["sketch"], "path_sketch": path["sketch"],
                "path": line["created"]["curves"], "orientation": "parallel", "operation": "new"
            }),
        )?
    } else {
        run(
            "solid.extrude",
            json!({"sketch": sketch["sketch"], "distance": width, "operation": "new"}),
        )?
    };
    if double_helix {
        mirror_helix(&body["created_bodies"], origin, span)?;
    }
    Ok(())
}

fn generate_involute_gear(params: &Value) -> Result<String, PluginError> {
    let target = params["replace_body"].as_str().unwrap_or("").trim();
    let teeth = tooth_count(params, "teeth", 24.0)?;
    let shift = profile_shift(params, "profile_shift")?;
    let module = number(params, "module", 2.0)?;
    let pressure = number(params, "pressure_angle", 20.0)?;
    let width = number(params, "face_width", 10.0)?;
    let fillet = number(params, "rim_fillet", 0.0)?;
    let root_round = root_fillet(params)?;
    if fillet < 0.0 || (width > 0.0 && fillet >= width / 2.0) {
        return Err(invalid(
            "rim_fillet",
            "The radius must be non-negative and smaller than half the face width.",
        ));
    }
    let bore = number(params, "bore_diameter", 8.0)?;
    let bore_shape = BoreShape::read(params, "", bore)?;
    let helix = number(params, "helix_angle", 0.0)?;
    let double_helix = params["double_helix"].as_bool() == Some(true);
    if helix.abs() >= 90.0 {
        return Err(invalid(
            "helix_angle",
            "Use a helix angle between -90° and 90°.",
        ));
    }
    let backlash = number(params, "backlash", 0.0)?;
    if backlash < 0.0 {
        return Err(invalid("backlash", "Backlash cannot be negative."));
    }
    if module <= 0.0 {
        return Err(invalid("module", "Module must be positive."));
    }
    if pressure <= 0.0 || pressure >= 90.0 {
        return Err(invalid(
            "pressure_angle",
            "Enter an angle between 0° and 90°.",
        ));
    }
    if width <= 0.0 {
        return Err(invalid("face_width", "Face width must be positive."));
    }

    let origin = [
        number(params, "position_x", 0.0)?,
        number(params, "position_y", 0.0)?,
        number(params, "position_z", 0.0)?,
    ];
    let rotation = number(params, "rotation", 0.0)?
        .rem_euclid(360.0)
        .to_radians();
    let generate_pair = match params.get("generate_pair") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or_else(|| invalid("generate_pair", "Expected a checkbox value."))?,
    };
    let tooth_allowance = if generate_pair {
        backlash / 2.0
    } else {
        backlash
    };
    let mating_teeth = if generate_pair {
        tooth_count(params, "mating_teeth", 48.0)?
    } else {
        0.0
    };
    let smooth = root_round == 0.0 && teeth + mating_teeth <= 80.0;
    if !target.is_empty() {
        replace_body(target)?;
    }
    let mut result = create_gear(
        teeth,
        module,
        pressure,
        width,
        (bore, "bore_diameter", bore_shape),
        (tooth_allowance, smooth, shift, fillet, root_round),
        origin,
        (rotation, helix, double_helix),
    )?;
    result["message"] = json!("SolidGear");

    if generate_pair {
        let mating_bore = number(params, "mating_bore_diameter", bore)?;
        let mating_shift = profile_shift(params, "mating_profile_shift")?;
        let distance =
            center_distance(module, pressure, teeth + mating_teeth, shift + mating_shift)?;
        let offset = polar(distance, rotation);
        let mating_origin = [origin[0] + offset[0], origin[1] + offset[1], origin[2]];
        // Aim a tooth of the first gear at the center of a gap of the second.
        let mating_rotation = rotation + PI - PI / mating_teeth;
        let mating = create_gear(
            mating_teeth,
            module,
            pressure,
            width,
            (
                mating_bore,
                "mating_bore_diameter",
                BoreShape::read(params, "mating_", mating_bore)?,
            ),
            (tooth_allowance, smooth, mating_shift, fillet, root_round),
            mating_origin,
            (mating_rotation, -helix, double_helix),
        )?;
        let mesh_check = if params["check_mesh"].as_bool() == Some(true) {
            check_mesh(&result, &mating, distance, pressure)
        } else {
            Value::Null
        };
        let ratio = mating_teeth / teeth;
        let rounded = format!("{ratio:.3}");
        let reduction = rounded.trim_end_matches('0').trim_end_matches('.');
        let summary = format!(
            "Ratio {reduction}:1 | Axes {distance:.3} mm | Outside diameters {:.3} / {:.3} mm",
            result["outer_diameter"].as_f64().unwrap(),
            mating["outer_diameter"].as_f64().unwrap()
        );
        log::log(log::Level::Info, &summary);
        result["message"] = json!(summary);
        result["pair"] = json!({
            "center_distance": distance,
            "ratio": ratio,
            "reduction": format!("{reduction}:1"),
            "mesh_check": mesh_check,
            "mating_gear": mating
        });
    }
    Ok(result.to_string())
}

fn tooth_count(params: &Value, field: &str, default: f64) -> Result<f64, PluginError> {
    let teeth = number(params, field, default)?;
    if teeth.fract() != 0.0 || !(3.0..=120.0).contains(&teeth) {
        return Err(invalid(field, "Enter a whole number from 3 to 120."));
    }
    Ok(teeth)
}

fn create_gear(
    teeth: f64,
    module: f64,
    pressure: f64,
    width: f64,
    bore: (f64, &str, BoreShape),
    profile: (f64, bool, f64, f64, f64),
    origin: [f64; 3],
    orientation: (f64, f64, bool),
) -> Result<Value, PluginError> {
    if origin.iter().any(|value| !value.is_finite()) {
        return Err(invalid(
            "position_x",
            "The resulting gear position is too large.",
        ));
    }
    let pitch = module * (teeth / 2.0);
    let (backlash, smooth, shift, fillet, root_round) = profile;
    let root = module * (teeth / 2.0 - 1.25 + shift);
    let tip = pitch + module * (1.0 + shift);
    if root <= 0.0 {
        return Err(invalid("profile_shift", "The shift leaves no root circle."));
    }
    if !(2.0 * tip).is_finite() {
        return Err(invalid("module", "The resulting diameter is too large."));
    }
    let (bore, bore_field, shape) = bore;
    let bore_extent = shape.extent(bore / 2.0);
    if bore < 0.0 || bore_extent >= root {
        return Err(invalid(
            bore_field,
            "The bore must fit inside the root circle.",
        ));
    }

    let pressure = pressure.to_radians();
    let base = pitch * pressure.cos();
    let start = base.max(root);
    let tooth_pitch = TAU / teeth;
    // Backlash is measured on the pitch circle. Each generated wheel removes
    // half the requested play for a pair; a standalone wheel removes it all.

    let thickness_offset = (backlash / 2.0 - shift * module * pressure.tan()) / pitch;
    let half_angle = |radius: f64| {
        let local_pressure = (base / radius).clamp(0.0, 1.0).acos();
        PI / (2.0 * teeth) + involute(pressure) - involute(local_pressure) - thickness_offset
    };
    let start_angle = half_angle(start);
    let tip_angle = half_angle(tip);
    if tip_angle <= 0.0 || start_angle >= tooth_pitch / 2.0 {
        return Err(invalid(
            "pressure_angle",
            "This angle and tooth count produce crossing tooth flanks.",
        ));
    }

    let (rotation, helix, double_helix) = orientation;
    let twist = (width / pitch * helix.to_radians().tan()).to_degrees();
    if !twist.is_finite() || twist.abs() > 1440.0 {
        return Err(invalid(
            "helix_angle",
            "This width and angle require more than four turns.",
        ));
    }
    let plane_at = |origin: [f64; 3], rotation: f64| {
        let (sin, cos) = rotation.sin_cos();
        json!({"origin": origin, "x_dir": [cos, sin, 0.0], "y_dir": [-sin, cos, 0.0]})
    };
    let straight_bore_cut = bore > 0.0 && twist != 0.0 && !matches!(shape, BoreShape::Round);
    let profile_at = |origin, rotation| -> Result<Value, PluginError> {
        let created = run(
            "sketch.create",
            json!({"plane": plane_at(origin, rotation)}),
        )?;
        // Large sketch patterns can exceed the native solver deadline.
        if smooth {
            let samples = 6;
            let flank = |side: f64| {
                (0..=samples)
                    .map(|sample| {
                        let radius = start + (tip - start) * sample as f64 / samples as f64;
                        polar(radius, side * half_angle(radius))
                    })
                    .collect::<Vec<_>>()
            };
            let left = flank(-1.0);
            let right = flank(1.0);
            let left_root = polar(root, -start_angle);
            let right_root = polar(root, start_angle);
            let mut curves = Vec::new();
            let mut add = |command, args| -> Result<(), PluginError> {
                let reply = run(command, args)?;
                let created = reply["created"]["curves"].as_array().ok_or_else(|| {
                    PluginError::Trap("Sketch command did not return curve IDs.".into())
                })?;
                curves.extend(created.iter().cloned());
                Ok(())
            };
            if root < start {
                add(
                    "sketch.line",
                    json!({"points": [left_root, left[0]], "infer": false}),
                )?;
            }
            add("sketch.spline.fit_point", json!({"points": left}))?;
            add(
                "sketch.arc.three_point",
                json!({"start": left[samples], "through": [tip, 0.0], "end": right[samples]}),
            )?;
            add("sketch.spline.fit_point", json!({"points": right}))?;
            if root < start {
                add(
                    "sketch.line",
                    json!({"points": [right[0], right_root], "infer": false}),
                )?;
            }
            add(
                "sketch.arc.three_point",
                json!({
                    "start": right_root,
                    "through": polar(root, tooth_pitch / 2.0),
                    "end": polar(root, tooth_pitch - start_angle)
                }),
            )?;
            run(
                "sketch.pattern.circular",
                json!({"entities": curves, "center": [0, 0], "count": teeth as u32}),
            )?;
        } else {
            let mut points = Vec::new();
            let mut corners = Vec::new();
            let segments = if root_round > 0.0 { 4 } else { 6 };
            for tooth in 0..teeth as usize {
                let center = tooth as f64 * tooth_pitch;
                corners.push(points.len());
                for side in [-1.0, 1.0] {
                    if side < 0.0 && root < start {
                        points.push(polar(root, center - start_angle));
                    }
                    for sample in 0..=segments {
                        let order = if side < 0.0 {
                            sample
                        } else {
                            segments - sample
                        };
                        let radius = start + (tip - start) * f64::from(order) / f64::from(segments);
                        points.push(polar(radius, center + side * half_angle(radius)));
                    }
                    if side < 0.0 {
                        points.push(polar(tip, center));
                    } else if root < start {
                        points.push(polar(root, center + start_angle));
                    }
                }
                corners.push(points.len() - 1);
                for sample in 1..3 {
                    let gap = tooth_pitch - 2.0 * start_angle;
                    points.push(polar(
                        root,
                        center + start_angle + gap * f64::from(sample) / 3.0,
                    ));
                }
            }
            let points = round_root_corners(points, &corners, root_round)?;
            run(
                "sketch.line",
                json!({"points": points, "closed": true, "infer": false, "connect": false}),
            )?;
        }

        if bore > 0.0 && !straight_bore_cut {
            shape.draw(bore)?;
        }
        run("sketch.finish", json!({}))?;
        Ok(created["sketch"].clone())
    };
    let sketch = profile_at(origin, rotation)?;
    let created_body = if twist == 0.0 {
        run(
            "solid.extrude",
            json!({
                "sketch": sketch, "distance": width, "operation": "new"
            }),
        )?
    } else {
        let swept = if double_helix {
            let half = width / 2.0;
            let first = sweep_gear(sketch, origin, half, twist / 2.0)?;
            mirror_helix(&first["created_bodies"], origin, half)?;
            first
        } else {
            sweep_gear(sketch, origin, width, twist)?
        };
        if straight_bore_cut {
            let bore_sketch = run(
                "sketch.create",
                json!({"plane": plane_at(origin, rotation)}),
            )?;
            shape.draw(bore)?;
            run("sketch.finish", json!({}))?;
            run(
                "solid.extrude",
                json!({
                    "sketch": bore_sketch["sketch"], "distance": width, "operation": "cut",
                    "targets": swept["created_bodies"]
                }),
            )?;
        }
        swept
    };

    let body = &created_body["created_bodies"][0];
    if fillet > 0.0 {
        round_rim(body, origin, width, tip, fillet)?;
    }
    Ok(json!({
        "body": body,
        "rim_fillet": fillet,
        "teeth": teeth,
        "backlash": backlash,
        "profile_shift": shift,
        "pitch_diameter": 2.0 * pitch,
        "outer_diameter": 2.0 * tip,
        "root_diameter": 2.0 * root,
        "helix_angle": helix,
        "double_helix": double_helix && twist != 0.0,
        "profile": if smooth { "spline-and-arc" } else { "segmented" }
    }))
}

fn round_rim(
    body: &Value,
    origin: [f64; 3],
    width: f64,
    tip: f64,
    radius: f64,
) -> Result<(), PluginError> {
    if radius >= tip {
        return Err(invalid(
            "rim_fillet",
            "The fillet must fit inside the outer radius.",
        ));
    }
    // Revolve a cutter with quarter-circle sections around the gear axis.
    // This rounds the outer diameter without blending every tooth flank.
    let sketch = run(
        "sketch.create",
        json!({
            "plane": {"origin": origin, "x_dir": [1, 0, 0], "y_dir": [0, 0, 1]}
        }),
    )?;
    let inner = tip - radius;
    let outer = tip + radius;
    let diagonal = radius * std::f64::consts::FRAC_1_SQRT_2;
    for (base, direction) in [(0.0, 1.0), (width, -1.0)] {
        let point = |x, y| [x, base + direction * y];
        run(
            "sketch.arc.three_point",
            json!({
                "start": point(inner, 0.0),
                "through": point(inner + diagonal, radius - diagonal),
                "end": point(tip, radius)
            }),
        )?;
        run(
            "sketch.line",
            json!({
                "points": [point(tip, radius), point(outer, radius), point(outer, -radius),
                    point(inner, -radius), point(inner, 0.0)],
                "infer": false
            }),
        )?;
    }
    run("sketch.finish", json!({}))?;
    run(
        "solid.revolve",
        json!({
            "sketch": sketch["sketch"], "axis": "y", "operation": "cut", "targets": [body]
        }),
    )?;
    Ok(())
}

fn mirror_helix(bodies: &Value, origin: [f64; 3], half: f64) -> Result<(), PluginError> {
    run(
        "solid.mirror",
        json!({
            "bodies": bodies, "combine": true,
            "plane": {"origin": [origin[0], origin[1], origin[2] + half], "normal": [0, 0, 1]}
        }),
    )?;
    Ok(())
}

fn sweep_gear(
    sketch: Value,
    origin: [f64; 3],
    width: f64,
    twist: f64,
) -> Result<Value, PluginError> {
    let path = run(
        "sketch.create",
        json!({
            "plane": {"origin": origin, "x_dir": [1, 0, 0], "y_dir": [0, 0, 1]}
        }),
    )?;
    let line = run(
        "sketch.line",
        json!({"points": [[0.0, 0.0], [0.0, width]], "infer": false}),
    )?;
    run("sketch.finish", json!({}))?;
    let args = json!({
        "sketch": sketch, "path_sketch": path["sketch"],
        "path": line["created"]["curves"], "twist": twist, "operation": "new"
    });
    run("solid.sweep", args)
}

#[derive(Clone, Copy)]
enum BoreShape {
    Round,
    Flat { x: f64 },
    Keyway { width: f64, depth: f64 },
}

impl BoreShape {
    fn read(params: &Value, prefix: &str, bore: f64) -> Result<Self, PluginError> {
        let field = format!("{prefix}bore_shape");
        let radius = bore / 2.0;
        if bore == 0.0 {
            return Ok(Self::Round);
        }
        match params.get(&field).map(Value::as_str) {
            None | Some(Some("round")) => Ok(Self::Round),
            Some(Some("d-flat")) => {
                let key = format!("{prefix}flat_offset");
                let x = number(params, &key, radius * 0.75)?;
                if x.abs() >= radius {
                    return Err(invalid(
                        &key,
                        "Flat offset must be between the two sides of the bore.",
                    ));
                }
                Ok(Self::Flat { x })
            }
            Some(Some("keyway")) => {
                let width_key = format!("{prefix}keyway_width");
                let depth_key = format!("{prefix}keyway_depth");
                let width = number(params, &width_key, radius / 2.0)?;
                let depth = number(params, &depth_key, radius / 4.0)?;
                if width <= 0.0 || width >= bore {
                    return Err(invalid(
                        &width_key,
                        "Keyway width must be positive and smaller than the bore.",
                    ));
                }
                if depth <= 0.0 {
                    return Err(invalid(&depth_key, "Keyway depth must be positive."));
                }
                Ok(Self::Keyway { width, depth })
            }
            _ => Err(invalid(&field, "Choose Round, D-flat or Keyway.")),
        }
    }

    fn extent(self, radius: f64) -> f64 {
        match self {
            Self::Keyway { width, depth } => (radius + depth).hypot(width / 2.0),
            _ => radius,
        }
    }

    fn draw(self, diameter: f64) -> Result<(), PluginError> {
        if matches!(self, Self::Round) {
            run(
                "sketch.circle.center",
                json!({"center": [0, 0], "diameter": diameter}),
            )?;
            return Ok(());
        }
        let radius = diameter / 2.0;
        let (x, y) = match self {
            Self::Flat { x } => (x, (radius * radius - x * x).sqrt()),
            Self::Keyway { width, .. } => (
                (radius * radius - (width / 2.0).powi(2)).sqrt(),
                width / 2.0,
            ),
            Self::Round => unreachable!(),
        };
        let start = [x, y];
        let end = [x, -y];
        run(
            "sketch.arc.three_point",
            json!({
                "start": start, "through": [-radius, 0.0], "end": end
            }),
        )?;
        let points = match self {
            Self::Flat { .. } => vec![end, start],
            Self::Keyway { depth, .. } => {
                vec![end, [radius + depth, -y], [radius + depth, y], start]
            }
            Self::Round => unreachable!(),
        };
        run("sketch.line", json!({"points": points, "infer": false}))?;
        Ok(())
    }
}

fn replace_body(name: &str) -> Result<(), PluginError> {
    let items = json!([{"type": "body", "name": name}]);
    let check = run("selection.delete", json!({"items": items, "dry_run": true}))?;
    if check["bodies"] != json!([name])
        || !check["would_fail"].as_array().is_some_and(Vec::is_empty)
        || !check["deleted_joints"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !check["skipped"].as_array().is_some_and(Vec::is_empty)
    {
        return Err(invalid(
            "replace_body",
            "The body could not be replaced on its own.",
        ));
    }
    let removed = run("selection.delete", json!({"items": items}))?;
    if removed["bodies"] != json!([name]) {
        return Err(invalid("replace_body", "The named body was not removed."));
    }
    Ok(())
}

fn profile_shift(params: &Value, field: &str) -> Result<f64, PluginError> {
    let shift = number(params, field, 0.0)?;
    if !(-0.5..=0.5).contains(&shift) {
        return Err(invalid(
            field,
            "Enter a profile shift coefficient from -0.5 to 0.5.",
        ));
    }
    Ok(shift)
}

fn center_distance(module: f64, pressure: f64, teeth: f64, shift: f64) -> Result<f64, PluginError> {
    let nominal = module * teeth / 2.0;
    if shift == 0.0 {
        return Ok(nominal);
    }
    let angle = pressure.to_radians();
    let target = involute(angle) + 2.0 * shift * angle.tan() / teeth;
    if target <= 0.0 {
        return Err(invalid(
            "mating_profile_shift",
            "The combined shift leaves no positive working pressure angle.",
        ));
    }
    let (mut low, mut high) = (0.0, PI / 2.0);
    for _ in 0..48 {
        let mid = (low + high) / 2.0;
        if involute(mid) < target {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok(nominal * angle.cos() / ((low + high) / 2.0).cos())
}

struct MeshProfile {
    count: usize,
    pitch: f64,
    root: f64,
    tip: f64,
    base: f64,
    half_at_base: f64,
}

impl MeshProfile {
    fn from_result(gear: &Value, pressure: f64) -> Self {
        let read = |key: &str| gear[key].as_f64().unwrap();
        let count = read("teeth") as usize;
        let pitch = read("pitch_diameter") / 2.0;
        let module = 2.0 * pitch / count as f64;
        Self {
            count,
            pitch,
            root: read("root_diameter") / 2.0,
            tip: read("outer_diameter") / 2.0,
            base: pitch * pressure.cos(),
            half_at_base: PI / (2.0 * count as f64)
                + involute(pressure)
                + (read("profile_shift") * module * pressure.tan() - read("backlash") / 2.0)
                    / pitch,
        }
    }

    fn half_angle(&self, radius: f64) -> f64 {
        self.half_at_base - involute((self.base / radius).clamp(0.0, 1.0).acos())
    }

    fn overlaps(&self, point: [f64; 2], rotation: f64, tolerance: f64) -> bool {
        let radius = point[0].hypot(point[1]);
        if radius < self.root - tolerance {
            return true;
        }
        if radius >= self.tip - tolerance {
            return false;
        }
        let period = TAU / self.count as f64;
        let angle =
            (point[1].atan2(point[0]) - rotation + period / 2.0).rem_euclid(period) - period / 2.0;
        (self.half_angle(radius) - angle.abs()) * radius > tolerance
    }

    fn outline(&self) -> Vec<[f64; 2]> {
        let period = TAU / self.count as f64;
        let mut points = Vec::new();
        for tooth in 0..self.count {
            let angle = tooth as f64 * period;
            for sample in 0..=16 {
                let fraction = f64::from(sample) / 16.0;
                let radius = self.root + (self.tip - self.root) * fraction;
                let flank = self.half_angle(radius);
                points.push(polar(radius, angle - flank));
                points.push(polar(radius, angle + flank));
                points.push(polar(
                    self.tip,
                    angle + (2.0 * fraction - 1.0) * self.half_angle(self.tip),
                ));
            }
        }
        points
    }
}

fn check_mesh(first: &Value, second: &Value, distance: f64, pressure: f64) -> Value {
    let pressure = pressure.to_radians();
    let a = MeshProfile::from_result(first, pressure);
    let b = MeshProfile::from_result(second, pressure);
    let a_points = a.outline();
    let b_points = b.outline();
    let samples = 24;
    let tolerance = a.pitch * 0.000_01;
    let mut interference_angles = Vec::new();
    for step in 0..samples {
        let turn = TAU * f64::from(step) / f64::from(samples);
        let ra = turn / a.count as f64;
        let rb = PI - (PI + turn) / b.count as f64;
        let crosses = |points: &[[f64; 2]],
                       rotation: f64,
                       offset: f64,
                       other: &MeshProfile,
                       other_rotation: f64| {
            let (sin, cos) = rotation.sin_cos();
            points.iter().any(|&[x, y]| {
                other.overlaps(
                    [x * cos - y * sin + offset, x * sin + y * cos],
                    other_rotation,
                    tolerance,
                )
            })
        };
        if crosses(&a_points, ra, -distance, &b, rb) || crosses(&b_points, rb, distance, &a, ra) {
            interference_angles.push(ra.to_degrees());
        }
    }
    let summary = format!(
        "Mesh check: interference at {} of {samples} sampled positions.",
        interference_angles.len()
    );
    log::log(log::Level::Info, &summary);
    json!({"message": summary, "samples": samples, "interference_angles_deg": interference_angles,
        "method": "sampled transverse tooth profiles"})
}

fn involute(angle: f64) -> f64 {
    angle.tan() - angle
}

fn polar(radius: f64, angle: f64) -> [f64; 2] {
    [radius * angle.cos(), radius * angle.sin()]
}

fn number(params: &Value, field: &str, default: f64) -> Result<f64, PluginError> {
    match params.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_f64()
            .ok_or_else(|| invalid(field, "Expected a number.")),
    }
}

fn invalid(field: &str, reason: impl Into<String>) -> PluginError {
    PluginError::InvalidInput(FieldFailure {
        field: field.into(),
        reason: reason.into(),
    })
}

fn run(command: &str, args: Value) -> Result<Value, PluginError> {
    let reply = commands::run(command, &args.to_string())?;
    serde_json::from_str(&reply).map_err(|error| PluginError::Trap(error.to_string()))
}

export!(SolidGear);
