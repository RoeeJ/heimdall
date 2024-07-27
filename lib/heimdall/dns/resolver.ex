defmodule Heimdall.DNS.Resolver do
  @moduledoc """
  Resolver for DNS.
  """

  require Logger
  alias Heimdall.Servers.Tracker
  alias Heimdall.DNS.{Model, Cache}
  alias Heimdall.Schema.Record
  alias Heimdall.Servers.Blocker

  @default_nameservers [
    {"1.1.1.1", 53},
    {"1.0.0.1", 53}
  ]

  def start_link(_) do
    {:ok,
     %{
       total_queries: 0,
       failed_queries: 0,
       blocked_queries: 0,
       cache_stats: %{}
     }}
  end

  def query(domain, type \\ :a, opts \\ []) do
    if type == :any do
      {:error, :any_not_supported}
    else
      cache_key = {domain, type}
      nameservers = Keyword.get(opts, :nameservers, @default_nameservers)
      opts = Keyword.put(opts, :nameservers, nameservers)

      case Blocker.filter_query(domain) do
        :blocked ->
          Tracker.report_blocked()
          publish_query(domain, type, :block)
          {:error, :blocked}

        :allowed ->
          Logger.debug("Query allowed for #{domain}")

          case Cache.get(cache_key) do
            {:ok, cached_resources}
            when not is_nil(cached_resources) and cached_resources != [] ->
              Tracker.report_success()
              publish_query(domain, type, :success)
              {:ok, cached_resources}

            {:partial, partial_resources, last_fetch_time} ->
              result =
                handle_partial_cache(partial_resources, last_fetch_time, domain, type, opts)

              publish_query(domain, type, :success)
              result

            _ ->
              result = handle_no_cache(domain, type, opts)
              handle_query_result(result, domain, type)
          end
      end
    end
  end

  defp handle_query_result(result, domain, type) do
    case result do
      {:ok, _} ->
        Tracker.report_success()
        publish_query(domain, type, :success)
        result

      {:error, _} ->
        Tracker.report_failed()
        publish_query(domain, type, :fail)
        result
    end
  end

  def stats(state) do
    state
  end

  # Internal Functions

  defp publish_query(domain, type, status) do
    Phoenix.PubSub.broadcast(Heimdall.PubSub, "queries", %{
      timestamp: DateTime.utc_now(),
      domain: domain,
      type: type,
      status: status
    })
  end

  defp handle_partial_cache(partial_resources, last_fetch_time, domain, type, opts) do
    if should_refresh?(last_fetch_time) do
      case refresh_records(domain, type, opts) do
        {:ok, fresh_resources} -> {:ok, fresh_resources}
        {:error, :nxdomain} -> {:error, :nxdomain}
        {:error, _} -> {:ok, partial_resources}
      end
    else
      {:ok, partial_resources}
    end
  end

  defp handle_no_cache(domain, type, opts) do
    Logger.debug("No cache for #{domain} (#{type})")

    case refresh_records(domain, type, opts) do
      {:ok, fresh_resources} -> {:ok, fresh_resources}
      {:error, :nxdomain} -> handle_nxdomain(domain, type, opts)
      {:error, reason} -> {:error, reason}
    end
  end

  defp handle_nxdomain(domain, type, opts) do
    if Application.get_env(:heimdall, :recursion, false) do
      Logger.debug("Recursion enabled, querying further for #{domain} (#{type})")

      case DNS.query(domain, type, opts) do
        {:ok, dns_record} ->
          resources = dns_record_to_resources(dns_record)
          Cache.put({domain, type}, resources)
          {:ok, resources}

        {:error, reason} ->
          {:error, reason}
      end
    else
      Logger.debug("Recursion not enabled, not querying further for #{domain} (#{type})")
      {:error, :nxdomain}
    end
  end

  defp should_refresh?(last_fetch_time) do
    now = System.system_time(:second)
    # Refresh if last fetch was more than 5 minutes ago
    now - last_fetch_time > 300
  end

  defp refresh_records(domain, type, _opts) do
    case Heimdall.DNS.Manager.query_subdomain(domain, type) do
      {:error, :nxdomain} ->
        Cache.put({domain, type}, [])
        {:error, :nxdomain}

      {:ok, records} when is_list(records) ->
        resources = Enum.map(records, &schema_record_to_resource(&1, domain))

        Cache.put({domain, type}, resources)

        {:ok, resources}
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

  defp record_to_resource(%Record{} = rec) do
    case rec do
      %{type: :a, data: %{"ip" => ip}} ->
        {:a, parse_ip(ip), 4}

      %{type: :aaaa, data: %{"ip" => ip}} ->
        {:aaaa, parse_ipv6(ip), 16}

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

  defp parse_ip(ip) do
    ip
    |> String.split(".")
    |> Enum.map(&String.to_integer/1)
    |> List.to_tuple()
  end

  defp parse_ipv6(ip) do
    expanded_ip = expand_ipv6_address(ip)

    expanded_ip
    |> String.split(":")
    |> Enum.map(&String.to_integer(&1, 16))
    |> List.to_tuple()
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

  defp calculate_rdlength(data) when is_binary(data), do: byte_size(data)
  defp calculate_rdlength(data) when is_list(data), do: length(data)
  defp calculate_rdlength(data) when is_tuple(data), do: tuple_size(data) * 4
  defp calculate_rdlength(_), do: 0

  defp parse_rdata(:a, {a, b, c, d}), do: {a, b, c, d}
  defp parse_rdata(:aaaa, {a, b, c, d, e, f, g, h}), do: {a, b, c, d, e, f, g, h}
  defp parse_rdata(:cname, data) when is_list(data), do: to_string(data)
  defp parse_rdata(:txt, data) when is_list(data), do: to_string(data)
  defp parse_rdata(:ns, data), do: to_string(data)
  defp parse_rdata(:srv, {pri, weight, port, target}), do: {pri, weight, port, to_string(target)}

  defp parse_rdata(:mx, {preference, exchange}) when is_integer(preference) and is_list(exchange),
    do: {preference, to_string(exchange)}

  defp parse_rdata(_, data), do: data

  defp update_stats(state, stat) do
    Map.update!(state, stat, &(&1 + 1))
  end
end
