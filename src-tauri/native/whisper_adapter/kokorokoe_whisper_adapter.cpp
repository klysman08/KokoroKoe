#include "kokorokoe_whisper_adapter.h"

#include "ggml-backend.h"
#include "whisper.h"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <new>
#include <string>
#include <utility>
#include <vector>

namespace {

constexpr uint32_t kApiVersion = 3;
constexpr size_t kMaximumSamples = 480000;
constexpr size_t kMaximumSegments = 256;
constexpr size_t kMaximumTextBytes = 1024 * 1024;
constexpr int32_t kBackendCpu = KK_WHISPER_BACKEND_CPU;
constexpr int32_t kBackendVulkan = KK_WHISPER_BACKEND_VULKAN;

struct Model {
    whisper_context *context = nullptr;
    std::string language = "auto";
    int32_t threads = 1;
    int32_t backend = kBackendCpu;

    ~Model() {
        if (context != nullptr) {
            whisper_free(context);
        }
    }
};

struct Segment {
    int64_t start_10ms = 0;
    int64_t end_10ms = 0;
    std::string text;
};

struct Result {
    std::string language;
    std::vector<Segment> segments;
};

void quiet_log_callback(enum ggml_log_level, const char *, void *) {}

bool valid_samples(const float *samples, size_t count) {
    if (samples == nullptr || count == 0 || count > kMaximumSamples) {
        return false;
    }
    for (size_t index = 0; index < count; ++index) {
        if (!std::isfinite(samples[index]) || samples[index] < -1.0F || samples[index] > 1.0F) {
            return false;
        }
    }
    return true;
}

bool backend_is_vulkan(ggml_backend_dev_t device) {
    const enum ggml_backend_dev_type device_type = ggml_backend_dev_type(device);
    if (device_type != GGML_BACKEND_DEVICE_TYPE_GPU &&
        device_type != GGML_BACKEND_DEVICE_TYPE_IGPU) {
        return false;
    }
    const ggml_backend_reg_t registry = ggml_backend_dev_backend_reg(device);
    const char *name = registry == nullptr ? nullptr : ggml_backend_reg_name(registry);
    return name != nullptr && _stricmp(name, "Vulkan") == 0;
}

bool vulkan_device_available() {
    for (size_t index = 0; index < ggml_backend_dev_count(); ++index) {
        if (backend_is_vulkan(ggml_backend_dev_get(index))) {
            return true;
        }
    }
    return false;
}

#if defined(KK_WHISPER_PROTOTYPE_FAULT_INJECTION)
bool prototype_should_abort_vulkan_inference() {
    char *value = nullptr;
    size_t length = 0;
    if (_dupenv_s(&value, &length, "KOKOROKOE_P3_005_ABORT_VULKAN_INFERENCE") != 0 ||
        value == nullptr) {
        return false;
    }
    const bool should_abort = std::strcmp(value, "1") == 0;
    std::free(value);
    return should_abort;
}
#endif

}  // namespace

uint32_t kk_whisper_api_version(void) {
    return kApiVersion;
}

int32_t kk_whisper_model_load(
    const char *model_path_utf8,
    const char *language_utf8,
    int32_t threads,
    int32_t backend,
    void **model_out) {
    if (model_out == nullptr) {
        return 1;
    }
    *model_out = nullptr;
    if (model_path_utf8 == nullptr || model_path_utf8[0] == '\0' || language_utf8 == nullptr ||
        language_utf8[0] == '\0' || threads < 1 || threads > 64 ||
        (backend != kBackendCpu && backend != kBackendVulkan)) {
        return 1;
    }
    const std::string language(language_utf8);
    if (language != "auto" && whisper_lang_id(language.c_str()) < 0) {
        return 1;
    }

    try {
        whisper_log_set(quiet_log_callback, nullptr);
        if (backend == kBackendVulkan && !vulkan_device_available()) {
            return 4;
        }
        whisper_context_params parameters = whisper_context_default_params();
        parameters.use_gpu = backend == kBackendVulkan;
        parameters.flash_attn = false;
        std::unique_ptr<Model> model(new Model{});
        model->context = whisper_init_from_file_with_params(model_path_utf8, parameters);
        if (model->context == nullptr) {
            return 2;
        }
        model->threads = threads;
        model->backend = backend;
        model->language = language;
        *model_out = model.release();
        return 0;
    } catch (...) {
        return 3;
    }
}

void kk_whisper_model_free(void *model) {
    delete static_cast<Model *>(model);
    whisper_log_set(nullptr, nullptr);
}

int32_t kk_whisper_model_backend(const void *model_pointer) {
    const auto *model = static_cast<const Model *>(model_pointer);
    return model == nullptr ? -1 : model->backend;
}

int32_t kk_whisper_transcribe(
    void *model_pointer,
    const float *samples,
    size_t sample_count,
    void **result_out) {
    if (result_out == nullptr) {
        return 1;
    }
    *result_out = nullptr;
    auto *model = static_cast<Model *>(model_pointer);
    if (model == nullptr || model->context == nullptr || !valid_samples(samples, sample_count)) {
        return 1;
    }

    try {
#if defined(KK_WHISPER_PROTOTYPE_FAULT_INJECTION)
        if (model->backend == kBackendVulkan && prototype_should_abort_vulkan_inference()) {
            std::abort();
        }
#endif
        whisper_full_params parameters = whisper_full_default_params(WHISPER_SAMPLING_GREEDY);
        parameters.n_threads = model->threads;
        parameters.translate = false;
        parameters.no_context = true;
        parameters.no_timestamps = false;
        parameters.single_segment = false;
        parameters.print_special = false;
        parameters.print_progress = false;
        parameters.print_realtime = false;
        parameters.print_timestamps = false;
        parameters.suppress_blank = true;
        parameters.suppress_nst = true;
        parameters.language = model->language.c_str();
        parameters.detect_language = false;

        const int status = whisper_full(
            model->context,
            parameters,
            samples,
            static_cast<int>(sample_count));
        if (status != 0) {
            return 2;
        }

        const int segment_count = whisper_full_n_segments(model->context);
        if (segment_count < 0 || static_cast<size_t>(segment_count) > kMaximumSegments) {
            return 3;
        }
        std::unique_ptr<Result> result(new Result{});
        const int language_id = whisper_full_lang_id(model->context);
        const char *language = whisper_lang_str(language_id);
        result->language = language == nullptr ? "und" : language;
        result->segments.reserve(static_cast<size_t>(segment_count));

        size_t total_text_bytes = 0;
        for (int index = 0; index < segment_count; ++index) {
            const char *text = whisper_full_get_segment_text(model->context, index);
            if (text == nullptr) {
                return 3;
            }
            Segment segment{
                whisper_full_get_segment_t0(model->context, index),
                whisper_full_get_segment_t1(model->context, index),
                text,
            };
            total_text_bytes += segment.text.size();
            if (total_text_bytes > kMaximumTextBytes) {
                return 3;
            }
            result->segments.push_back(std::move(segment));
        }
        *result_out = result.release();
        return 0;
    } catch (...) {
        return 4;
    }
}

void kk_whisper_result_free(void *result) {
    delete static_cast<Result *>(result);
}

const char *kk_whisper_result_language(const void *result_pointer) {
    const auto *result = static_cast<const Result *>(result_pointer);
    return result == nullptr ? nullptr : result->language.c_str();
}

size_t kk_whisper_result_segment_count(const void *result_pointer) {
    const auto *result = static_cast<const Result *>(result_pointer);
    return result == nullptr ? 0 : result->segments.size();
}

int64_t kk_whisper_result_segment_start_10ms(const void *result_pointer, size_t index) {
    const auto *result = static_cast<const Result *>(result_pointer);
    if (result == nullptr || index >= result->segments.size()) {
        return -1;
    }
    return result->segments[index].start_10ms;
}

int64_t kk_whisper_result_segment_end_10ms(const void *result_pointer, size_t index) {
    const auto *result = static_cast<const Result *>(result_pointer);
    if (result == nullptr || index >= result->segments.size()) {
        return -1;
    }
    return result->segments[index].end_10ms;
}

const char *kk_whisper_result_segment_text(const void *result_pointer, size_t index) {
    const auto *result = static_cast<const Result *>(result_pointer);
    if (result == nullptr || index >= result->segments.size()) {
        return nullptr;
    }
    return result->segments[index].text.c_str();
}
