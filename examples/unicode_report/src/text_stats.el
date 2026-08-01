defmodule TextStats do
  @derive [Eq, Show]
  defstruct Summary do
    text: string
    byte_count: usize
    codepoint_count: usize
    grapheme_count: usize
    unique_graphemes: usize
    frequencies: Map(string, i32)
  end

  def count_from_some(value: {:some, i32}) -> i32 do
    match value do
      {:some, count} -> count
    end
  end

  def increment(frequencies: Map(string, i32), grapheme: string) -> Map(string, i32) do
    match Map.fetch(frequencies, grapheme) do
      found: {:some, i32} ->
        Map.put(frequencies, grapheme, count_from_some(found) + 1)
      :none -> Map.put(frequencies, grapheme, 1)
    end
  end

  def analyze(text: string) -> Summary do
    graphemes = text |> String.graphemes()
    frequencies: Map(string, i32) = Enum.reduce(graphemes, Map.new(), increment)
    %Summary{
      text: text,
      byte_count: String.byte_size(text),
      codepoint_count: Enum.count(String.codepoint_view(text)),
      grapheme_count: Enum.count(graphemes),
      unique_graphemes: Map.size(frequencies),
      frequencies: frequencies
    }
  end
end
