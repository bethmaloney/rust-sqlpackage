CREATE TABLE [dbo].[BinaryData] (
    [Id] INT NOT NULL PRIMARY KEY,
    [SmallData] VARBINARY(100) NULL,
    [LargeData] VARBINARY(MAX) NULL
);
