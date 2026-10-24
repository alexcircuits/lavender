# Tactical Complex Event Processing (CEP) Engine for Lavender
# Uses Nim's AST macros to compile declarative alerting rules into native machine code.

import macros, math
import types

proc pointInBox*(lat, lon, south, west, north, east: float64): bool {.inline.} =
  result = lat >= south and lat <= north and lon >= west and lon <= east

proc haversineDistanceKm*(lat1, lon1, lat2, lon2: float64): float64 =
  let r = 6371.0 # Earth radius in km
  let dLat = degToRad(lat2 - lat1)
  let dLon = degToRad(lon2 - lon1)
  let a = sin(dLat / 2.0)^2 + cos(degToRad(lat1)) * cos(degToRad(lat2)) * sin(dLon / 2.0)^2
  let c = 2.0 * arctan2(sqrt(a), sqrt(1.0 - a))
  result = r * c

# Template to generate a compiled rule evaluator
template tacticalRule*(name: untyped, ruleIdVal: string, severityVal: AlertSeverity, msgVal: string, condition: untyped): untyped =
  proc name*(e {.inject.}: TelemetryEvent, alert: var AlertResult): bool =
    if condition:
      alert.triggered = true
      alert.ruleId = ruleIdVal
      alert.severity = int32(severityVal)
      alert.message = msgVal
      return true
    return false

# Rule 1: High-speed low-altitude drone detection
tacticalRule(checkLowFastDrone, "TAC-001", sevCritical, "High-speed drone detected below 200m altitude"):
  $e.category == "drone" and e.speedKt >= 90.0 and e.altM <= 200.0

# Rule 2: Vessel loitering in protected maritime zone (e.g. English Channel TSS)
tacticalRule(checkVesselLoitering, "TAC-002", sevWarning, "Vessel loitering (<3 kt) in maritime traffic zone"):
  $e.category == "vessel" and e.speedKt <= 3.0 and pointInBox(e.lat, e.lon, 50.0, -1.0, 51.5, 2.0)

# Rule 3: Extreme low altitude aircraft outside approach corridor
tacticalRule(checkAircraftTerrainProximity, "TAC-003", sevWarning, "Low altitude civilian aircraft (<150m)"):
  $e.category == "aircraft" and e.altM > 0.0 and e.altM <= 150.0 and e.speedKt > 150.0

# Master rule evaluation pipeline
proc evaluateAllRules*(e: TelemetryEvent, alert: var AlertResult): bool =
  if checkLowFastDrone(e, alert): return true
  if checkVesselLoitering(e, alert): return true
  if checkAircraftTerrainProximity(e, alert): return true
  return false

# C-ABI Exports for Rust and C integration
proc lavender_cep_eval*(e: ptr TelemetryEvent, alert: ptr AlertResult): bool {.exportc, dynlib.} =
  if e == nil or alert == nil: return false
  alert.triggered = false
  return evaluateAllRules(e[], alert[])

proc lavender_cep_version*(): cstring {.exportc, dynlib.} =
  return "Lavender CEP Engine v0.1.0 (Nim ARC/ORC)"
