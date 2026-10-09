#pragma once
#include "rust/cxx.h"
struct ModelJob;
void startModelThread(rust::Box<ModelJob> job);
void joinModelThreads();
