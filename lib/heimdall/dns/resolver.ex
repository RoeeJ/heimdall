defmodule Heimdall.DNS.Resolver do
  use GenServer
  alias Heimdall.DNS.Model

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
    nameservers = Keyword.get(opts, :nameservers, state.nameservers)
    query_opts = Keyword.put(opts, :nameservers, nameservers)

    case DNS.query(domain, type, query_opts) do
      {:ok, dns_record} ->
        resources = dns_record_to_resources(dns_record)
        {:reply, {:ok, resources}, state}

      {:error, reason} ->
        {:reply, {:error, reason}, state}
    end
  end

  defp dns_record_to_resources(dns_record) do
    Enum.map(dns_record.anlist, &dns_rr_to_heimdall/1)
  end

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
       when is_integer(preference) and is_list(exchange) do
    {preference, to_string(exchange)}
  end

  defp parse_rdata(_, data), do: data
end
