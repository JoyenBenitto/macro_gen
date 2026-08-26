cmake_minimum_required(VERSION 3.15)
project(macro_gen VERSION 1.0.0 LANGUAGES CXX)

# Force the use of standard modern C++17
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Optional: Add compiler flags to catch coding errors early
if(NOT MSVC)
    add_compile_options(-Wall -Wextra -Wpedantic -O2)
endif()

add_executable(macro_gen 
    src/main.cpp
)

target_include_directories(macro_gen PRIVATE 
    ${CMAKE_CURRENT_SOURCE_DIR}/include
)
