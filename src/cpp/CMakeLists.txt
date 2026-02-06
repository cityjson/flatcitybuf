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

# Build Rust static library
add_custom_command(
    OUTPUT ${RUST_LIB_DIR}/target/release/libfcb_cpp.a
    COMMAND cargo build --release -p fcb_cpp
    WORKING_DIRECTORY ${RUST_LIB_DIR}
    DEPENDS ${RUST_LIB_DIR}/fcb_cpp/src/lib.rs
    DEPENDS ${RUST_LIB_DIR}/fcb_cpp/src/reader.rs
    DEPENDS ${RUST_LIB_DIR}/fcb_cpp/src/writer.rs
    COMMENT "Building Rust fcb_cpp library"
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
endif()

# Example executable
add_executable(fcb_example examples/example.cpp ${GENERATED_DIR}/lib.rs.h)
target_link_libraries(fcb_example fcb)
target_include_directories(fcb_example PRIVATE ${GENERATED_DIR})

# Install targets
install(TARGETS fcb ARCHIVE DESTINATION lib)
install(FILES ${GENERATED_DIR}/lib.rs.h DESTINATION include/fcb)
