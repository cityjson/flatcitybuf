find_package(doctest CONFIG REQUIRED)

add_executable(fcb_tests
    doctest_main.cpp
    test_error.cpp
    test_generated_schema.cpp
    test_layout.cpp
    test_range_reader.cpp
)
target_link_libraries(fcb_tests PRIVATE fcb_core_cpp doctest::doctest)
target_include_directories(fcb_tests PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
target_compile_definitions(fcb_tests PRIVATE
    FCB_TEST_DATA_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../../../examples/data"
)

include(CTest)
add_test(NAME fcb_tests COMMAND fcb_tests)
