defmodule Main do
  def default_text() -> string do
    "Hello, élan! 🇸🇬 🙂🙂 👩‍👩‍👧‍👦"
  end

  def first_argument(arguments: [string]) -> string do
    match arguments do
      [first | _] -> first
      [] -> default_text()
    end
  end

  def arguments_from_ok(value: {:ok, [string]}) -> [string] do
    match value do
      {:ok, arguments} -> arguments
    end
  end

  def input_text() -> string do
    match Process.arguments() do
      value: {:ok, [string]} -> first_argument(arguments_from_ok(value))
      value: {:error, {:invalid_text, usize}} -> default_text()
    end
  end

  def main() -> i32 do
    summary = TextStats.analyze(input_text())
    Presentation.print(summary)
    0
  end
end
