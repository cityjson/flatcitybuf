cmake_minimum_required(VERSION 3.16)
project(flatcitybuf VERSION 0.8.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

option(FCB_WITH_JSON   "Build the CityJSON conversion component" ON)
option(FCB_WITH_CURL   "Build the libcurl HTTP range reader"     OFF)
option(FCB_BUILD_TESTS "Build tests"                             ON)

find_package(flatbuffers CONFIG REQUIRED)

add_library(fcb_core_cpp STATIC
    src/layout.cpp
    src/range_reader.cpp
    src/header.cpp
    src/reader.cpp
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
endif()

if(FCB_WITH_CURL)
    find_package(CURL REQUIRED)
    target_link_libraries(fcb_core_cpp PUBLIC CURL::libcurl)
    target_compile_definitions(fcb_core_cpp PUBLIC FCB_WITH_CURL=1)
endif()

if(FCB_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
