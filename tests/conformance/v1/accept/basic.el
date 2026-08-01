# conformance: accept; spec=EXAMPLES.md §1-5, GRAMMAR.md §3, TYPES.md §7
defmodule Main do
  def main() -> i32 do
    left: i32 = 20
    right: i32 = 22
    if left < right do
      left + right
    else
      0
    end
  end
end
