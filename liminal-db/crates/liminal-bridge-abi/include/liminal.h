#ifndef LIMINAL_BRIDGE_H
#define LIMINAL_BRIDGE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Initialize the LiminalDB core bridge.
 *
 * The configuration buffer must contain CBOR encoded data with at least
 * the key { "tick_ms": <u32> } specifying the simulation tick interval
 * in milliseconds.
 */
bool liminal_init(const uint8_t *cfg_cbor, size_t len);

/**
 * Submit an impulse into the core.
 *
 * The message buffer is a CBOR encoded Impulse:
 * { "k":0|1|2, "p":text, "s":float32, "t":uint32, "tg":[text] }.
 * Returns the number of bytes consumed or 0 on error.
 */
size_t liminal_push(const uint8_t *msg_cbor, size_t len);

/**
 * Retrieve pending events and metrics.
 *
 * The output buffer is filled with a CBOR encoded package:
 * { "events":[Event...], "metrics":Metrics? }.
 * Event objects use compact keys: { "ev":text, "id":uint|text,
 * "dt":uint32, "meta":{...} }. Metrics objects use
 * { "cells":uint32, "sleeping":float32, "avgMet":float32, "avgLat":uint32 }.
 * Returns the number of bytes written or 0 if nothing is available or
 * the buffer is too small.
 */
size_t liminal_pull(uint8_t *out, size_t cap);

#ifdef __cplusplus
}
#endif

#endif /* LIMINAL_BRIDGE_H */
