-- Junction table linking instruments to tags (for nested alias tests)
CREATE TABLE [dbo].[InstrumentTag]
(
    [Id] INT NOT NULL PRIMARY KEY,
    [InstrumentId] INT NOT NULL,
    [TagId] INT NOT NULL,
    CONSTRAINT [FK_InstrumentTag_Instrument] FOREIGN KEY ([InstrumentId]) REFERENCES [dbo].[Instrument] ([Id]),
    CONSTRAINT [FK_InstrumentTag_Tag] FOREIGN KEY ([TagId]) REFERENCES [dbo].[Tag] ([Id])
);
GO
