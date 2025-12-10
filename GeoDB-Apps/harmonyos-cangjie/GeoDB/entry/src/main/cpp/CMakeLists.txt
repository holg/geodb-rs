# CMakeLists.txt for GeoDB NAPI bridge
cmake_minimum_required(VERSION 3.4.1)
project(geodb_napi)

set(NATIVERENDER_ROOT_PATH ${CMAKE_CURRENT_SOURCE_DIR})
set(GEODB_LIBS_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../../libs/${OHOS_ARCH})

# Add NAPI headers
include_directories(${NATIVERENDER_ROOT_PATH}
                    ${NATIVERENDER_ROOT_PATH}/include)

# Add library search path BEFORE add_library
link_directories(${GEODB_LIBS_DIR})

# Add our NAPI bridge source
add_library(geodb_napi SHARED
    geodb_napi.cpp
)

# Link against libace_napi (HarmonyOS NAPI library)
target_link_libraries(geodb_napi PUBLIC libace_napi.z.so)

# Link Rust library by name only (not full path)
target_link_libraries(geodb_napi PUBLIC geodb_ffi)
