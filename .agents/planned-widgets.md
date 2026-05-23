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

| Data Type               | Sport                        | fit-parse                               | GPX parser                                                   | Derivation                    | Status       |
| ----------------------- | ---------------------------- | --------------------------------------- | ------------------------------------------------------------ | ----------------------------- | ------------ |
| **Pace**                | Run, Swim                    | ✅ `pace`                               | ✅ `pace`                                                    | Fallback: 1000/speed          | ✅ Derived   |
| **G-Force**             | Motor, Cycle, Ski, Paraglide | ✅ `g_force`, `gforce`                  | ✅ `g_force`, `gforce`                                       | No                            | ✅ Extracted |
| **Air Pressure**        | Hike, Paraglide, Ski         | ✅ `absolute_pressure`                  | ✅ `air_pressure`, `pressure`                                | No                            | ✅ Extracted |
| **Ground Contact Time** | Run                          | ✅ `ground_contact_time`, `stance_time` | ✅ `ground_contact_time`, `groundcontacttime`, `stance_time` | No                            | ✅ Extracted |
| **Left/Right Balance**  | Run                          | ✅ `left_right_balance`                 | ✅ `left_right_balance`, `balance`                           | No                            | ✅ Extracted |
| **Stride Length**       | Run                          | ✅ `stride_length`, `step_length`       | ✅ `stride_length`, `stridelength`, `step_length`            | No                            | ✅ Extracted |
| **Stroke Rate**         | Swim, Row                    | ✅ `stroke_rate`, `running_cadence`     | ✅ `stroke_rate`, `strokerate`                               | No                            | ✅ Extracted |
| **Torque**              | Cycle                        | ✅ `torque`                             | ✅ `torque`                                                  | Fallback: P/(2π × cadence/60) | ✅ Derived   |
| **Vertical Speed**      | Paraglide, Hike, Cycle       | ✅ `vertical_speed`                     | ✅ `vertical_speed`, `verticalspeed`, `vam`                  | Fallback: Δele/Δt             | ✅ Derived   |

### New parser fields needed

| Data Type            | Sport        | fit-parse             | GPX parser | Derivation | Status        |
| -------------------- | ------------ | --------------------- | ---------- | ---------- | ------------- |
| **Gear Position**    | Cycle, Motor | ✅ `gear`             | ❌ rare    | No         | ❌ Not parsed |
| **Vertical Ratio**   | Run          | ✅ `vertical_ratio`   | ❌         | No         | ❌ Not parsed |
| **Core Temperature** | All (heat)   | ✅ `core_temperature` | ❌         | No         | ❌ Not parsed |

---

## Advanced Graphical Widget (visual representation)

| Data Type                     | Sport                               | fit-parse                                                                              | GPX parser                                       | Derivation                                     | Status                 |
| ----------------------------- | ----------------------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------ | ---------------------------------------------- | ---------------------- |
| **Heading / Compass Needle**  | Motor, Cycle, Hike, Sail, Paraglide | ✅ `gps_heading`, `compass_heading`, `heading`, `course_heading`, `navigation_heading` | ✅ `heading`, `course`, `bearing`, `gps_heading` | Fallback: bearing between course points        | ✅ Extracted + derived |
| **G-Meter** (lat/lon G split) | Motor, Ski, MTB                     | G-Force is scalar; vector needs derivation                                             | ❌                                               | ✅ split from g-force + heading + speed deltas | ❌ Not derived         |

---
