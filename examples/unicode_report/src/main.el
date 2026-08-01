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

  def input_text() -> string do
    match Process.arguments() do
      {:ok, arguments} -> first_argument(arguments)
      {:error, {:invalid_text, _}} -> default_text()
    end
  end

  def main() -> i32 do
    summary = TextStats.analyze(input_text())
    Presentation.print(summary)
    0
  end
end
