// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc, char **argv) {
  if (argc != 2) return 2;
  void *h = dlopen("/Applications/Anki.app/Contents/Frameworks/Python.framework/Versions/3.13/Python", RTLD_NOW | RTLD_GLOBAL);
  if (!h) { puts(dlerror()); return 1; }
  void (*init)(void) = dlsym(h, "Py_Initialize");
  int (*run)(FILE *, const char *, int, void *) = dlsym(h, "PyRun_SimpleFileExFlags");
  if (!init || !run) return 1;
  FILE *f = fopen(argv[1], "r");
  if (!f) return 1;
  init();
  return run(f, argv[1], 1, NULL) ? 1 : 0;
}
