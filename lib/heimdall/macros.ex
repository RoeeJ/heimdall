defmodule Heimdall.Macros do
  defmacro const(const_name, const_value) do
    quote do
      def unquote(const_name)(), do: unquote(const_value)
    end
  end

  defmacro b2b(bool) do
    quote do
      case unquote(bool) do
        true -> 1
        false -> 0
      end
    end
  end
end
