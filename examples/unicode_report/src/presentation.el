defmodule Presentation do
  def string_from_ok(value: {:ok, string}) -> string do
    match value do
      {:ok, text} -> text
    end
  end

  def banner() -> string do
    buffer = Buffer.new()
    titled = Buffer.append_string(buffer, "EL v1")
    separated = Buffer.append_bytes(titled, String.bytes(" · "))
    complete = Buffer.append_string(separated, "Unicode report")
    match Buffer.to_string(complete) do
      value: {:ok, string} -> string_from_ok(value)
      value: {:error, String.Utf8Error} -> "Unicode report"
    end
  end

  def print_frequency(entry: {string, i32}) -> unit do
    match entry do
      {grapheme, count} ->
        IO.print("  ")
        IO.print(grapheme)
        IO.print(": ")
        IO.println(Format.positive_i32(count))
    end
  end

  def print(summary: TextStats.Summary) -> unit do
    IO.println(banner())
    IO.println("Input:")
    IO.println(summary.text)
    IO.println("UTF-8 bytes:")
    IO.println(Format.decimal(summary.byte_count))
    IO.println("Unicode code points:")
    IO.println(Format.decimal(summary.codepoint_count))
    IO.println("Extended grapheme clusters:")
    IO.println(Format.decimal(summary.grapheme_count))
    IO.println("Unique grapheme clusters:")
    IO.println(Format.decimal(summary.unique_graphemes))
    IO.println("Frequency map (first-seen order):")
    Enum.each(summary.frequencies, print_frequency)
  end
end
