// Hand-authored Phase 0 oracle. Not transpiler output. Workspace license applies.
public final class Counter {
    private int count = 0;
    private int revision = 0;
    private void add(int amount) { count += amount; revision++; }
    public static void main(String[] args) {
        Counter counter = new Counter();
        for (int amount : new int[] {1, 0, 2}) {
            counter.add(amount);
            System.out.println(counter.revision + "|" + counter.count + "|" + counter.count);
        }
    }
}
