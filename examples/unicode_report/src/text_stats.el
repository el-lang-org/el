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

  def increment(frequencies: Map(string, i32), grapheme: string) -> Map(string, i32) do
    match Map.fetch(frequencies, grapheme) do
      {:some, frequency} ->
        Map.put(frequencies, grapheme, frequency + 1)
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
