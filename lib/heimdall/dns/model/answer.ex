defmodule Heimdall.DNS.Model.Answer do
  def parse(answers, data, 0), do: [answers, data]

  def parse(answers, data, count) do
  end
end
