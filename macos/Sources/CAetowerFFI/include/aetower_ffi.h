#ifndef AETOWER_FFI_H
#define AETOWER_FFI_H

#include <stdint.h>

void *aetower_engine_new(void);
void aetower_engine_start(void *handle);
void aetower_engine_stop(void *handle);
void aetower_engine_free(void *handle);
char *aetower_engine_get_snapshot_json(void *handle);
void aetower_engine_set_capability_state(void *handle, const char *kind, const char *state, const char *detail);
void aetower_engine_free_string(char *value);

#endif
