## UNRELATED

-- test this format yuv420p10le for high fidelity; yuv444p or yuv444p10le if alpha is needed (but beware of compatibility issues with some players)

# Planned Widgets — Data Type Research

Cross-sport analysis of per-sample dynamic data types that could be implemented as widgets.

## Legend

- **Simple Metric** = numeric value display (like current Speed/HR/Power widgets)
- **Advanced Graphic** = visual representation (compass needle, gauge, etc.)
- **Derived** = must be computed from other fields (Y = always, Fallback = direct preferred but derived as fallback)
- **Extracted** = currently pulled from file or computed during parse workflow

---

## Simple Metric Widget (numeric value display)

### Already parsed/derived, no widget yet

| Data Type               | Icon         | Sport                        | fit-parse                               | GPX parser                                                   | Derivation                    | Status       | Unambiguous  | Unit choices                          |
| ----------------------- | ------------ | ---------------------------- | --------------------------------------- | ------------------------------------------------------------ | ----------------------------- | ------------ | ------------ | ------------------------------------- |
| **Pace**                | `Footprints` | Run, Swim                    | ✅ `pace`                               | ✅ `pace`                                                    | Fallback: 1000/speed          | ✅ Derived   | ✅ Yes       | min/km, min/mi                        |
| **G-Force**             | Custom       | Motor, Cycle, Ski, Paraglide | ✅ `g_force`, `gforce`                  | ✅ `g_force`, `gforce`                                       | No                            | ✅ Extracted | ✅ Yes       | g, m/s²                               |
| **Air Pressure**        | `Wind`       | Hike, Paraglide, Ski         | ✅ `absolute_pressure`                  | ✅ `air_pressure`, `pressure`                                | No                            | ✅ Extracted | ✅ Yes       | hPa, mbar, inHg, mmHg                 |
| **Ground Contact Time** | Custom       | Run                          | ✅ `ground_contact_time`, `stance_time` | ✅ `ground_contact_time`, `groundcontacttime`, `stance_time` | No                            | ✅ Extracted | ✅ Yes       | ms                                    |
| **Left/Right Balance**  | `Scale`      | Run                          | ✅ `left_right_balance`                 | ✅ `left_right_balance`, `balance`                           | No                            | ✅ Extracted | ❌ Ambiguous | %, ratio (L/R split or 50/50 display) |
| **Stride Length**       | `Ruler`      | Run                          | ✅ `stride_length`, `step_length`       | ✅ `stride_length`, `stridelength`, `step_length`            | No                            | ✅ Extracted | ✅ Yes       | m, cm, ft, in                         |
| **Stroke Rate**         | `Waves`      | Swim, Row                    | ✅ `stroke_rate`, `running_cadence`     | ✅ `stroke_rate`, `strokerate`                               | No                            | ✅ Extracted | ✅ Yes       | spm                                   |
| **Torque**              | Custom       | Cycle                        | ✅ `torque`                             | ✅ `torque`                                                  | Fallback: P/(2π × cadence/60) | ✅ Derived   | ✅ Yes       | Nm, lb-ft                             |
| **Vertical Speed**      | `TrendingUp` | Paraglide, Hike, Cycle       | ✅ `vertical_speed`                     | ✅ `vertical_speed`, `verticalspeed`, `vam`                  | Fallback: Δele/Δt             | ✅ Derived   | ✅ Yes       | m/s, ft/min, m/h (VAM)                |

### New parser fields needed

| Data Type            | Icon          | Sport        | fit-parse             | GPX parser | Derivation | Status        | Unambiguous | Unit choices      |
| -------------------- | ------------- | ------------ | --------------------- | ---------- | ---------- | ------------- | ----------- | ----------------- |
| **Gear Position**    | Custom        | Cycle, Motor | ✅ `gear`             | ❌ rare    | No         | ❌ Not parsed | ✅ Yes      | unitless (gear #) |
| **Vertical Ratio**   | `Percent`     | Run          | ✅ `vertical_ratio`   | ❌         | No         | ❌ Not parsed | ✅ Yes      | %, ratio          |
| **Core Temperature** | `Thermometer` | All (heat)   | ✅ `core_temperature` | ❌         | No         | ❌ Not parsed | ✅ Yes      | °C, °F            |

---

## Advanced Graphical Widget (visual representation)

| Data Type                     | Icon          | Sport                               | fit-parse                                                                              | GPX parser                                       | Derivation                                     | Status                 |
| ----------------------------- | ------------- | ----------------------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------ | ---------------------------------------------- | ---------------------- |
| **Heading / Compass Needle**  | `Compass`     | Motor, Cycle, Hike, Sail, Paraglide | ✅ `gps_heading`, `compass_heading`, `heading`, `course_heading`, `navigation_heading` | ✅ `heading`, `course`, `bearing`, `gps_heading` | Fallback: bearing between course points        | ✅ Extracted + derived |
| **G-Meter** (lat/lon G split) | `CircleGauge` | Motor, Ski, MTB                     | G-Force is scalar; vector needs derivation                                             | ❌                                               | ✅ split from g-force + heading + speed deltas | ❌ Not derived         |

---
