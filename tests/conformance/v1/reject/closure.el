# conformance: reject; phase=syntax; code=E1000; spec=EXAMPLES.md §3, GRAMMAR.md §3
defmodule Main do
  def main() -> i32 do
    callback = fn value -> value + 1 end
    callback(41)
  end
end
