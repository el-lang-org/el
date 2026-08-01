# conformance: accept; spec=EXAMPLES.md §10-12, TYPES.md §6
defmodule Main do
  @derive [Eq]
  defstruct Box(a) when a: Eq do
    value: a
  end

  def same(a: Box(i32), b: Box(i32)) -> bool do
    a == b
  end

  def main() -> i32 do
    if same(%Box{value: 42}, %Box{value: 42}) do
      42
    else
      0
    end
  end
end
