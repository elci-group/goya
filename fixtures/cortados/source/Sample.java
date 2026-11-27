// Cortados Phase 1 fixture. Workspace MIT OR Apache-2.0 license applies.
package example;

import java.io.IOException;
import java.util.List;

sealed interface Shape permits Circle, Box { int area(); }
record Circle(int radius) implements Shape {
    public int area() { return radius * radius; }
}
final class Box implements Shape {
    private int size;
    Box(int size) { this.size = size; }
    public int area() { return size * size; }
}

class Sample {
    private int count;
    private final List<String> labels = List.of("one", "two");
    synchronized int add(int amount) throws IOException {
        try {
            count += amount;
            return switch (count) { case 0 -> 0; default -> count; };
        } finally {
            System.out.println("changed");
        }
    }
    String describe(Object value) {
        if (value instanceof String text) return text;
        return """
            Java 17 inventory
            """;
    }
}
