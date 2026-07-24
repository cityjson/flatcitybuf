cmake_minimum_required(VERSION 3.16)
project(flatcitybuf VERSION 0.8.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

option(FCB_WITH_JSON   "Build the CityJSON conversion component" ON)
option(FCB_WITH_CURL   "Build the libcurl HTTP range reader"     OFF)
option(FCB_BUILD_TESTS "Build tests"                             ON)
option(FCB_BUILD_EXAMPLES "Build example programs"               ON)

find_package(flatbuffers CONFIG REQUIRED)

add_library(fcb_core_cpp STATIC
    src/layout.cpp
    src/range_reader.cpp
    src/header.cpp
    src/reader.cpp
    src/packed_rtree.cpp
    src/key.cpp
    src/attribute.cpp
    src/geometry.cpp
    src/cityjson.cpp
    src/stree.cpp
)

target_include_directories(fcb_core_cpp PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)
target_include_directories(fcb_core_cpp PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/src
)
target_link_libraries(fcb_core_cpp PUBLIC flatbuffers::flatbuffers)

if(FCB_WITH_JSON)
    find_package(nlohmann_json 3.2.0 CONFIG REQUIRED)
    target_link_libraries(fcb_core_cpp PUBLIC nlohmann_json::nlohmann_json)
    target_compile_definitions(fcb_core_cpp PUBLIC FCB_WITH_JSON=1)
    target_sources(fcb_core_cpp PRIVATE src/writer/attribute.cpp src/writer/geom_encoder.cpp
                                        src/writer/feature_serializer.cpp
                                        src/writer/header_serializer.cpp)
endif()

if(FCB_WITH_CURL)
    find_package(CURL REQUIRED)
    target_sources(fcb_core_cpp PRIVATE src/http/curl_range_reader.cpp)
    target_link_libraries(fcb_core_cpp PUBLIC CURL::libcurl)
    target_compile_definitions(fcb_core_cpp PUBLIC FCB_WITH_CURL=1)
endif()

if(FCB_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()

if(FCB_BUILD_EXAMPLES)
    # Each example is one self-contained .cpp covering one capability; see
    # examples/README.md. read_features and query_attributes need the CityJSON
    # component for nlohmann::json rendering, so they are gated on it.
    add_executable(fcb_read_local examples/read_local.cpp)
    target_link_libraries(fcb_read_local PRIVATE fcb_core_cpp)

    add_executable(fcb_read_http examples/read_http.cpp)
    target_link_libraries(fcb_read_http PRIVATE fcb_core_cpp)

    add_executable(fcb_inspect_header examples/inspect_header.cpp)
    target_link_libraries(fcb_inspect_header PRIVATE fcb_core_cpp)

    add_executable(fcb_query_attributes examples/query_attributes.cpp)
    target_link_libraries(fcb_query_attributes PRIVATE fcb_core_cpp)

    add_executable(fcb_custom_reader examples/custom_reader.cpp)
    target_link_libraries(fcb_custom_reader PRIVATE fcb_core_cpp)

    if(FCB_WITH_JSON)
        add_executable(fcb_read_features examples/read_features.cpp)
        target_link_libraries(fcb_read_features PRIVATE fcb_core_cpp)
    endif()
endif()

# ------------------------------------------------------------- install ---

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

add_library(flatcitybuf::flatcitybuf ALIAS fcb_core_cpp)
set_target_properties(fcb_core_cpp PROPERTIES EXPORT_NAME flatcitybuf)

install(TARGETS fcb_core_cpp EXPORT flatcitybufTargets
        ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR})

# Public headers, including generated/, which already lives under include/
# so the same include path works in-tree and installed.
install(DIRECTORY include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})

install(EXPORT flatcitybufTargets
        NAMESPACE flatcitybuf::
        DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/flatcitybuf)

configure_package_config_file(
    cmake/flatcitybufConfig.cmake.in
    "${CMAKE_CURRENT_BINARY_DIR}/flatcitybufConfig.cmake"
    INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/flatcitybuf)
write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/flatcitybufConfigVersion.cmake"
    VERSION ${PROJECT_VERSION} COMPATIBILITY SameMajorVersion)
install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/flatcitybufConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/flatcitybufConfigVersion.cmake"
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/flatcitybuf)
