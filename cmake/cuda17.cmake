# CUDA 13+ CCCL headers (CUB, Thrust, libcu++) require C++17.
set(CMAKE_CUDA_STANDARD 17 CACHE STRING "CUDA C++ standard" FORCE)
set(CMAKE_CUDA_STANDARD_REQUIRED ON CACHE BOOL "" FORCE)

# CUDA 13+ dropped compute_52/61. Set architectures compatible with modern CUDA.
set(CMAKE_CUDA_ARCHITECTURES "70;75;80;86;89;90" CACHE STRING "CUDA architectures" FORCE)
