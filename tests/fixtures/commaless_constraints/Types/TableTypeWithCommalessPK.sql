-- Table type with primary key constraint lacking comma separator
CREATE TYPE [dbo].[TableTypeWithCommalessPK] AS TABLE
(
    [ElementId] INT NOT NULL,
    [SequenceNo] INT NULL,
    [ParentId] INT,
    [Name] NVARCHAR(200),
    [Value] NVARCHAR(MAX) NOT NULL
    PRIMARY KEY ([ElementId])
);
GO
