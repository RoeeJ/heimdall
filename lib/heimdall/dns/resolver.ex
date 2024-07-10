defmodule Heimdall.DNS.Resolver do
  require Logger

  def resolve(hostname, record_type) do
    case DNS.resolve(hostname, record_type, nameservers: [{"8.8.4.4", 53}]) do
      {:ok, res} ->
        {:ok, res}

      {:error, err} ->
        {:error, err}
    end
  end

  def query(hostname, record_type) do
    case DNS.query(hostname, record_type, nameservers: [{"8.8.4.4", 53}]) do
      {:ok, res} ->
        {:ok, res}

      {:error, err} ->
        {:error, err}
    end
  end
end
