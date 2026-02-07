CREATE TABLE [dbo].[Archive]
(
    [ArchiveId] INT NOT NULL,
    [Data] NVARCHAR(MAX) NOT NULL,
    [ArchivedDate] DATETIME2 NOT NULL,
    [Category] NVARCHAR(50) NOT NULL
);
