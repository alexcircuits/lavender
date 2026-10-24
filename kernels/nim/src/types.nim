# Telemetry types and Alert structures for Lavender CEP Engine

type
  TelemetryEvent* {.bycopy.} = object
    entityId*: cstring
    lat*: cdouble
    lon*: cdouble
    altM*: cdouble
    speedKt*: cdouble
    headingDeg*: cdouble
    timestampMs*: int64
    category*: cstring # "aircraft", "vessel", "drone", "hazard"

  AlertSeverity* = enum
    sevInfo = 0,
    sevWarning = 1,
    sevCritical = 2

  AlertResult* {.bycopy.} = object
    triggered*: bool
    ruleId*: cstring
    severity*: int32
    message*: cstring
