{
  producer = {
    kind = "source";
    inputs = { };
    outputs.generated = {
      type = "directory";
    };
  };

  consumer = {
    kind = "application";
    inputs.generated = {
      fromProject = "producer";
      output = "generated";
    };
    outputs.firmware = {
      type = "file";
    };
  };
}
