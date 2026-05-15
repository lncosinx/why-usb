cmake_minimum_required(VERSION 3.10)
project(why_usb_vhci CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED True)

add_library(why_usb_vhci STATIC src/vhci.cpp)
target_include_directories(why_usb_vhci PUBLIC inc)

install(TARGETS why_usb_vhci DESTINATION lib)
install(DIRECTORY inc/ DESTINATION include)
