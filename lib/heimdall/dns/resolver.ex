defmodule Heimdall.DNS.Resolver do
  @moduledoc """
  Resolver for DNS.
  """

  use GenServer
  require Logger
  alias Heimdall.DNS.{Model, Cache}
  alias Heimdall.Schema.Record

  @default_nameservers [
    {"1.1.1.1", 53},
    {"1.0.0.1", 53}
  ]

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def init(opts) do
    nameservers = Keyword.get(opts, :nameservers, @default_nameservers)
    {:ok, %{nameservers: nameservers}}
  end

  def query(domain, type \\ :a, opts \\ []) do
    GenServer.call(__MODULE__, {:query, domain, type, opts})
  end

  def handle_call({:query, domain, type, opts}, _from, state) do
    cache_key = {domain, type}

    case Cache.get(cache_key) do
      {:ok, cached_resources} when not is_nil(cached_resources) and cached_resources != [] ->
        {:reply, {:ok, cached_resources}, state}

      {:partial, partial_resources, last_fetch_time} ->
        handle_partial_cache(partial_resources, last_fetch_time, domain, type, opts, state)

      _ ->
        handle_no_cache(domain, type, opts, state)
    end
  end

  defp handle_partial_cache(partial_resources, last_fetch_time, domain, type, opts, state) do
    if should_refresh?(last_fetch_time) do
      case refresh_records(domain, type, opts) do
        {:ok, fresh_resources} -> {:reply, {:ok, fresh_resources}, state}
        {:error, _} -> {:reply, {:ok, partial_resources}, state}
      end
    else
      {:reply, {:ok, partial_resources}, state}
    end
  end

  defp handle_no_cache(domain, type, opts, state) do
    case refresh_records(domain, type, opts) do
      {:ok, fresh_resources} -> {:reply, {:ok, fresh_resources}, state}
      {:error, reason} -> {:reply, {:error, reason}, state}
    end
  end

  defp should_refresh?(last_fetch_time) do
    now = System.system_time(:second)
    # Refresh if last fetch was more than 5 minutes ago
    now - last_fetch_time > 300
  end

  defp refresh_records(domain, type, opts) do
    nameservers = Keyword.get(opts, :nameservers, @default_nameservers)
    query_opts = Keyword.put(opts, :nameservers, nameservers)

    Logger.debug("Refreshing records for #{domain} (#{type})")

    case Heimdall.DNS.Manager.query_subdomain(domain, type) do
      {:error, _} ->
        case DNS.query(domain, type, query_opts) do
          {:ok, dns_record} ->
            resources = dns_record_to_resources(dns_record)
            Cache.put({domain, type}, resources)

            {:ok, resources}

          {:error, reason} ->
            {:error, reason}
        end

      {:ok, records} when is_list(records) ->
        resources = Enum.map(records, &schema_record_to_resource(&1, domain))

        Cache.put({domain, type}, resources)

        {:ok, resources}
    end
  end

  defp record_to_resource(%Record{} = rec) do
    case rec do
      %{type: :a, data: %{"ip" => ip}} ->
        {:a,
         ip
         |> String.split(".")
         |> Enum.map(&String.to_integer/1)
         |> List.to_tuple(), 4}

      %{type: :aaaa, data: %{"ip" => ip}} ->
        expanded_ip = expand_ipv6_address(ip)

        {:aaaa,
         expanded_ip
         |> String.split(":")
         |> Enum.map(&String.to_integer(&1, 16))
         |> List.to_tuple(), 16}

      %{type: :cname, data: %{"host" => host}} ->
        {:cname, host, String.length(host)}

      %{type: :ns, data: %{"host" => host}} ->
        {:ns, host, String.length(host)}

      %{type: :mx, data: %{"host" => host, "preference" => preference}} ->
        {:mx, {preference, host}, String.length(host)}

      _ ->
        raise "Unsupported record type: #{rec.type}"
    end
  end

  defp expand_ipv6_address(address) do
    parts = String.split(address, ":")
    double_colon_index = Enum.find_index(parts, &(&1 == ""))

    if double_colon_index do
      before_dc = Enum.take(parts, double_colon_index)
      after_dc = Enum.drop(parts, double_colon_index + 1)
      missing_parts = 8 - (length(before_dc) + length(after_dc))

      (before_dc ++ List.duplicate("0000", missing_parts) ++ after_dc)
      |> Enum.map_join(":", &String.pad_leading(&1, 4, "0"))
    else
      parts
      |> Enum.map_join(":", &String.pad_leading(&1, 4, "0"))
    end
  end

  defp schema_record_to_resource(rec, domain) do
    {qtype, data, datalength} = record_to_resource(rec)

    %Model.ResourceRecord{
      qname: domain,
      qtype: qtype,
      qclass: :in,
      rdata: data,
      rdlength: datalength,
      ttl: rec.ttl
    }
  end

  defp dns_record_to_resources(dns_record), do: Enum.map(dns_record.anlist, &dns_rr_to_heimdall/1)

  defp dns_rr_to_heimdall(rr) do
    %Model.ResourceRecord{
      qname: to_string(rr.domain),
      qtype: rr.type,
      qclass: rr.class,
      ttl: rr.ttl,
      rdlength: calculate_rdlength(rr.data),
      rdata: parse_rdata(rr.type, rr.data)
    }
  end

  # Assuming string
  defp calculate_rdlength(data) when is_binary(data), do: byte_size(data)
  # Assuming charlist
  defp calculate_rdlength(data) when is_list(data), do: length(data)
  # Assuming IPv4 address
  defp calculate_rdlength(data) when is_tuple(data), do: tuple_size(data) * 4
  # Default case for other types
  defp calculate_rdlength(_), do: 0

  defp parse_rdata(:a, {a, b, c, d}), do: {a, b, c, d}
  defp parse_rdata(:aaaa, {a, b, c, d, e, f, g, h}), do: {a, b, c, d, e, f, g, h}
  defp parse_rdata(:cname, data) when is_list(data), do: to_string(data)
  defp parse_rdata(:txt, data) when is_list(data), do: to_string(data)
  defp parse_rdata(:ns, data), do: to_string(data)
  defp parse_rdata(:srv, {pri, weight, port, target}), do: {pri, weight, port, to_string(target)}

  defp parse_rdata(:mx, {preference, exchange})
       when is_integer(preference) and is_list(exchange),
       do: {preference, to_string(exchange)}

  defp parse_rdata(_, data), do: data
end
