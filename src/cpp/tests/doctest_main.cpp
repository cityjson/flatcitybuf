// The single translation unit that provides doctest's implementation and
// main(). Defining DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN in every test TU
// instead would emit the runtime 351 times over and fail to link.
#define DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN
#include <doctest/doctest.h>
