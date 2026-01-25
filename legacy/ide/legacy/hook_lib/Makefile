# Semantic Hook Library Makefile
#
# Builds libsemantic_hook.so for LD_PRELOAD syscall interception

CC = gcc
CFLAGS = -shared -fPIC -Wall -O2
LDFLAGS = -ldl

TARGET = libsemantic_hook.so
SOURCE = semantic_hook.c

# Installation paths
PREFIX ?= /usr/local
LIBDIR ?= $(PREFIX)/lib/semantic_os

.PHONY: all clean install uninstall test

all: $(TARGET)

$(TARGET): $(SOURCE)
	$(CC) $(CFLAGS) -o $@ $< $(LDFLAGS)

clean:
	rm -f $(TARGET)

install: $(TARGET)
	mkdir -p $(DESTDIR)$(LIBDIR)
	install -m 755 $(TARGET) $(DESTDIR)$(LIBDIR)/
	@echo "Installed to $(DESTDIR)$(LIBDIR)/$(TARGET)"

uninstall:
	rm -f $(DESTDIR)$(LIBDIR)/$(TARGET)

# Test with a simple command
test: $(TARGET)
	@echo "Testing hook library with 'cat /etc/hostname'..."
	@SEMANTIC_HOOK_SOCKET=/tmp/test_hook.sock \
		LD_PRELOAD=./$(TARGET) cat /etc/hostname 2>/dev/null || \
		echo "(Socket not listening - that's OK for this test)"
	@echo "Test complete. Check /tmp/test_hook.sock for output."

# Debug build with symbols
debug: CFLAGS += -g -DDEBUG
debug: $(TARGET)
