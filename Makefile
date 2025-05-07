CXX      = $(shell fltk-config --cxx)
DEBUG    = -g
CXXFLAGS = $(shell fltk-config --cxxflags ) -I.
LDFLAGS  = $(shell fltk-config --ldflags )
LDSTATIC = $(shell fltk-config --ldstaticflags )
LINK     = $(CXX)

TARGET = main 
OBJS   = main.o
SRCS   = main.cxx

.SUFFIXES: .o .cxx

%.o: %.cxx
	$(CXX) $(CXXFLAGS) $(DEBUG) -c $<

all: $(TARGET)

$(TARGET): $(OBJS)
	$(LINK) -o $(TARGET) $(OBJS) $(LDSTATIC)

curve.o: main.cxx

clean:
	rm -f *.o 2> /dev/null
	rm -f $(TARGET) 2> /dev/null
