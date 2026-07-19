find_package(doctest CONFIG REQUIRED)

add_executable(fcb_tests
    doctest_main.cpp
    test_error.cpp
    test_generated_schema.cpp
    test_layout.cpp
    test_range_reader.cpp
    test_header.cpp
    test_sequential_scan.cpp
    test_packed_rtree.cpp
    test_keys.cpp
    test_attributes.cpp
    test_geometry.cpp
    test_cityjson.cpp
    test_http.cpp
    test_stree.cpp
)
target_link_libraries(fcb_tests PRIVATE fcb_core_cpp doctest::doctest)
target_include_directories(fcb_tests PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
target_compile_definitions(fcb_tests PRIVATE
    FCB_TEST_DATA_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../../../examples/data"
)

include(CTest)
add_test(NAME fcb_tests COMMAND fcb_tests)

if(FCB_WITH_CURL)
    find_package(Python3 COMPONENTS Interpreter REQUIRED)
    # python3 -m http.server cannot serve Range requests, so the suite ships
    # its own range-capable server; see tests/range_server.py.
    add_test(NAME fcb_http_tests
             COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/run_http_tests.sh
                     ${Python3_EXECUTABLE}
                     ${CMAKE_CURRENT_SOURCE_DIR}/range_server.py
                     ${CMAKE_CURRENT_SOURCE_DIR}/../../../examples/data
                     $<TARGET_FILE:fcb_tests>)
endif()
