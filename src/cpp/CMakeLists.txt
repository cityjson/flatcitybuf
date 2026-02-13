cmake_minimum_required(VERSION 3.16)
project(fcb_cpp VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Find cxxbridge CLI tool
find_program(CXXBRIDGE cxxbridge)
if(NOT CXXBRIDGE)
    message(STATUS "cxxbridge not found, installing via cargo...")
    execute_process(
        COMMAND cargo install cxxbridge-cmd
        RESULT_VARIABLE CXXBRIDGE_INSTALL_RESULT
    )
    find_program(CXXBRIDGE cxxbridge REQUIRED)
endif()

# Configuration
set(RUST_LIB_DIR ${CMAKE_SOURCE_DIR}/../rust)
set(GENERATED_DIR ${CMAKE_BINARY_DIR}/generated)
file(MAKE_DIRECTORY ${GENERATED_DIR})

# Build Rust static library (without http feature to avoid OpenSSL dependency)
add_custom_command(
    OUTPUT ${RUST_LIB_DIR}/target/release/libfcb_cpp.a
    COMMAND cargo build --release -p fcb_cpp --no-default-features
    WORKING_DIRECTORY ${RUST_LIB_DIR}
    DEPENDS ${RUST_LIB_DIR}/fcb_cpp/src/lib.rs
    DEPENDS ${RUST_LIB_DIR}/fcb_cpp/src/reader.rs
    DEPENDS ${RUST_LIB_DIR}/fcb_cpp/src/writer.rs
    COMMENT "Building Rust fcb_cpp library (without http feature)"
)

add_custom_target(rust_library DEPENDS ${RUST_LIB_DIR}/target/release/libfcb_cpp.a)

# Generate C++ bridge code
add_custom_command(
    OUTPUT ${GENERATED_DIR}/lib.rs.h
    COMMAND ${CXXBRIDGE} ${RUST_LIB_DIR}/fcb_cpp/src/lib.rs --header > ${GENERATED_DIR}/lib.rs.h
    DEPENDS ${RUST_LIB_DIR}/fcb_cpp/src/lib.rs
    COMMENT "Generating C++ header from CXX bridge"
)

add_custom_command(
    OUTPUT ${GENERATED_DIR}/lib.rs.cc
    COMMAND ${CXXBRIDGE} ${RUST_LIB_DIR}/fcb_cpp/src/lib.rs > ${GENERATED_DIR}/lib.rs.cc
    DEPENDS ${RUST_LIB_DIR}/fcb_cpp/src/lib.rs
    COMMENT "Generating C++ source from CXX bridge"
)

# Create fcb library
add_library(fcb STATIC
    ${GENERATED_DIR}/lib.rs.cc
)

add_dependencies(fcb rust_library)

target_include_directories(fcb PUBLIC
    ${GENERATED_DIR}
    ${CMAKE_SOURCE_DIR}/include
)

target_link_libraries(fcb
    ${RUST_LIB_DIR}/target/release/libfcb_cpp.a
)

# Platform-specific linking
if(APPLE)
    target_link_libraries(fcb
        "-framework Security"
        "-framework CoreFoundation"
        "-framework SystemConfiguration"
    )
elseif(UNIX)
    # Linux: link against OpenSSL if the Rust library was built with http feature
    find_package(OpenSSL)
    if(OPENSSL_FOUND)
        target_link_libraries(fcb OpenSSL::SSL OpenSSL::Crypto)
        message(STATUS "Linking with OpenSSL: ${OPENSSL_VERSION}")
    endif()
endif()

# Find nlohmann/json (required for comprehensive_example)
find_package(nlohmann_json 3.2.0 QUIET)
if(NOT nlohmann_json_FOUND)
    message(WARNING "nlohmann/json not found - comprehensive_example requires it")
    message(STATUS "Install with: brew install nlohmann-json")
endif()

# Example executables
add_executable(fcb_example examples/example.cpp ${GENERATED_DIR}/lib.rs.h)
target_link_libraries(fcb_example fcb)
target_include_directories(fcb_example PRIVATE ${GENERATED_DIR})

# Local file operations example (no HTTP dependencies)
add_executable(local_fcb_example examples/local_fcb_example.cpp ${GENERATED_DIR}/lib.rs.h)
target_link_libraries(local_fcb_example fcb)
target_include_directories(local_fcb_example PRIVATE ${GENERATED_DIR})

add_executable(fcb_comprehensive_example examples/comprehensive_example.cpp ${GENERATED_DIR}/lib.rs.h)
target_link_libraries(fcb_comprehensive_example fcb)
target_include_directories(fcb_comprehensive_example PRIVATE ${GENERATED_DIR})

# Roundtrip test (encode CityJSONSeq -> FCB -> read back)
add_executable(fcb_roundtrip_test examples/roundtrip_test.cpp ${GENERATED_DIR}/lib.rs.h)
target_link_libraries(fcb_roundtrip_test fcb)
target_include_directories(fcb_roundtrip_test PRIVATE ${GENERATED_DIR})

# Link nlohmann/json for comprehensive_example if found
if(nlohmann_json_FOUND)
    target_link_libraries(fcb_comprehensive_example nlohmann_json::nlohmann_json)
endif()

# Install targets
install(TARGETS fcb ARCHIVE DESTINATION lib)
install(FILES ${GENERATED_DIR}/lib.rs.h DESTINATION include/fcb)

# Documentation (Doxygen)
find_package(Doxygen QUIET)
if(DOXYGEN_FOUND)
    add_custom_target(docs
        COMMAND ${DOXYGEN_EXECUTABLE} ${CMAKE_SOURCE_DIR}/Doxyfile
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
        COMMENT "Generating API documentation with Doxygen"
        DEPENDS ${GENERATED_DIR}/lib.rs.h
    )
    message(STATUS "Doxygen found - 'make docs' available")
else()
    message(STATUS "Doxygen not found - install with 'brew install doxygen' for docs")
endif()
