#pragma once

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#define KK_WHISPER_API __declspec(dllexport)
#else
#define KK_WHISPER_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

KK_WHISPER_API uint32_t kk_whisper_api_version(void);
KK_WHISPER_API int32_t kk_whisper_model_load(
    const char *model_path_utf8,
    int32_t threads,
    void **model_out);
KK_WHISPER_API void kk_whisper_model_free(void *model);
KK_WHISPER_API int32_t kk_whisper_transcribe(
    void *model,
    const float *samples,
    size_t sample_count,
    void **result_out);
KK_WHISPER_API void kk_whisper_result_free(void *result);
KK_WHISPER_API const char *kk_whisper_result_language(const void *result);
KK_WHISPER_API size_t kk_whisper_result_segment_count(const void *result);
KK_WHISPER_API int64_t kk_whisper_result_segment_start_10ms(
    const void *result,
    size_t index);
KK_WHISPER_API int64_t kk_whisper_result_segment_end_10ms(
    const void *result,
    size_t index);
KK_WHISPER_API const char *kk_whisper_result_segment_text(
    const void *result,
    size_t index);

#ifdef __cplusplus
}
#endif
