# Unit tests for Nim CEP Tactical Alerting Engine

import unittest
import types, engine

suite "Nim Tactical CEP Rules":

  test "detects high-speed low-altitude drone (TAC-001)":
    var e = TelemetryEvent(
      entityId: "DRONE-SHA-136",
      lat: 48.5,
      lon: 35.0,
      altM: 120.0,
      speedKt: 110.0,
      headingDeg: 270.0,
      timestampMs: 1700000000000,
      category: "drone"
    )
    var alert: AlertResult
    let triggered = evaluateAllRules(e, alert)

    check triggered == true
    check alert.ruleId == "TAC-001"
    check alert.severity == int32(sevCritical)
    check alert.message == "High-speed drone detected below 200m altitude"

  test "detects vessel loitering in traffic separation corridor (TAC-002)":
    var e = TelemetryEvent(
      entityId: "MMSI-211234567",
      lat: 50.8,
      lon: 1.2,
      altM: 0.0,
      speedKt: 1.5,
      headingDeg: 45.0,
      timestampMs: 1700000000000,
      category: "vessel"
    )
    var alert: AlertResult
    let triggered = evaluateAllRules(e, alert)

    check triggered == true
    check alert.ruleId == "TAC-002"
    check alert.severity == int32(sevWarning)

  test "passes normal cruising civil aircraft without false positive":
    var e = TelemetryEvent(
      entityId: "KLM1023",
      lat: 52.3,
      lon: 4.8,
      altM: 10500.0,
      speedKt: 460.0,
      headingDeg: 180.0,
      timestampMs: 1700000000000,
      category: "aircraft"
    )
    var alert: AlertResult
    let triggered = evaluateAllRules(e, alert)

    check triggered == false
    check alert.triggered == false

  test "C-ABI export lavender_cep_eval evaluates pointer structs":
    var e = TelemetryEvent(
      entityId: "DRONE-TEST",
      lat: 50.0,
      lon: 30.0,
      altM: 80.0,
      speedKt: 95.0,
      headingDeg: 120.0,
      timestampMs: 1700000000000,
      category: "drone"
    )
    var alert: AlertResult
    let ok = lavender_cep_eval(addr e, addr alert)

    check ok == true
    check alert.triggered == true
    check alert.ruleId == "TAC-001"
