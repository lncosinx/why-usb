cmake_minimum_required(VERSION 3.10)
project(why_usb_vhci CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED True)

set(DRIVER_SOURCES src/vhci.cpp)

if(WIN32)
    list(APPEND DRIVER_SOURCES src/driver.cpp src/device.cpp)
endif()

add_library(why_usb_vhci STATIC ${DRIVER_SOURCES})
target_include_directories(why_usb_vhci PUBLIC inc)

include(CTest)

if(BUILD_TESTING)
    add_executable(ring_buffer_tests tests/ring_buffer_tests.cpp)
    target_include_directories(ring_buffer_tests PRIVATE inc)
    add_test(NAME ring_buffer_tests COMMAND ring_buffer_tests)
endif()

install(TARGETS why_usb_vhci DESTINATION lib)
install(DIRECTORY inc/ DESTINATION include)
