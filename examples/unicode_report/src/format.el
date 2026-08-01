defmodule Format do
  def string_from_ok(value: {:ok, string}) -> string do
    match value do
      {:ok, text} -> text
    end
  end

  def append_digits(buffer: Buffer, value: usize) -> Buffer do
    if value >= 10 do
      prefix = append_digits(buffer, value / 10)
      Buffer.append_byte(prefix, u8(value % 10) + 48)
    else
      Buffer.append_byte(buffer, u8(value) + 48)
    end
  end

  def decimal(value: usize) -> string do
    buffer = append_digits(Buffer.new(), value)
    match Buffer.to_string(buffer) do
      result: {:ok, string} -> string_from_ok(result)
      result: {:error, String.Utf8Error} -> "?"
    end
  end

  def positive_i32(value: i32) -> string do
    decimal(usize(value))
  end
end
