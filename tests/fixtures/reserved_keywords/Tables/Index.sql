-- Table with reserved keyword 'Index'
CREATE TABLE [dbo].[Index] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Column] NVARCHAR(100) NOT NULL,   -- Reserved keyword as column
    [Group] NVARCHAR(100) NULL,        -- Reserved keyword as column
    [Level] INT NOT NULL DEFAULT 0     -- Reserved keyword as column
);
GO
