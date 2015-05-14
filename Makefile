CFLAGS ?= -O2 -g -Wall
LDFLAGS ?=

GIO_CFLAGS = $(shell pkg-config --cflags gio-2.0)
GIO_LDFLAGS = $(shell pkg-config --libs gio-2.0)

all: reflector

clean:
	rm -f reflector

reflector: reflector.c
	$(CC) -o $@ $^ $(CFLAGS) $(GIO_CFLAGS) $(LDFLAGS) $(GIO_LDFLAGS)

