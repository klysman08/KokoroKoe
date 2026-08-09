cmake_minimum_required(VERSION 3.25)
project(kokorokoe_whisper_adapter LANGUAGES C CXX)

if(NOT WIN32)
  message(FATAL_ERROR "The P3-004 adapter prototype targets Windows x64 only")
endif()
if(NOT DEFINED WHISPER_CPP_SOURCE_DIR OR NOT EXISTS "${WHISPER_CPP_SOURCE_DIR}/CMakeLists.txt")
  message(FATAL_ERROR "Set WHISPER_CPP_SOURCE_DIR to the pinned whisper.cpp source tree")
endif()

set(BUILD_SHARED_LIBS ON CACHE BOOL "" FORCE)
set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
set(GGML_VULKAN OFF CACHE BOOL "" FORCE)
set(WHISPER_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(WHISPER_BUILD_SERVER OFF CACHE BOOL "" FORCE)
set(WHISPER_BUILD_TESTS OFF CACHE BOOL "" FORCE)
add_subdirectory("${WHISPER_CPP_SOURCE_DIR}" whisper.cpp EXCLUDE_FROM_ALL)

add_library(kokorokoe_whisper_adapter SHARED kokorokoe_whisper_adapter.cpp)
target_compile_features(kokorokoe_whisper_adapter PRIVATE cxx_std_17)
target_include_directories(
  kokorokoe_whisper_adapter
  PRIVATE "${WHISPER_CPP_SOURCE_DIR}/include"
)
target_link_libraries(kokorokoe_whisper_adapter PRIVATE whisper)
target_compile_definitions(kokorokoe_whisper_adapter PRIVATE NOMINMAX WIN32_LEAN_AND_MEAN)
target_compile_options(kokorokoe_whisper_adapter PRIVATE /W4 /WX /permissive-)
set_target_properties(
  kokorokoe_whisper_adapter
  PROPERTIES
    CXX_EXTENSIONS OFF
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin/Release"
    RUNTIME_OUTPUT_DIRECTORY_RELEASE "${CMAKE_BINARY_DIR}/bin/Release"
)
